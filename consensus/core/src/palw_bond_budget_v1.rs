//! **ADR-0176 / ADR-0177: the bond budget and the model-bond allocation** — `palw_bond_budget_v1` and
//! `palw_model_bond_allocation_v1` (lane BUDGET; **dormant, no height**; design `docs/design/palw/bond-budget-and-model-allocation.md`).
//!
//! ```text
//! acceptance   reservation = ask (Q 1, B blocks, R reward, F Final weight), clipped by the per-claim ceilings when the policy slices
//!              rights by rho, and R by the model's available budget past the allocation fence
//!              admitted iff open_claims < cap and window + reservation <= caps(C) in all four dimensions
//! every writer consume(claim, dim, ask) grants min(ask, reserved - consumed); the rest is never minted / never credited
//! the clock    a reservation leaves the window at reuse_not_before = accepted_daa + W, and at no other time
//! allocation   S_m = sum_b C_{b,m} (signed tag 140, seasoned, pro-rata clipped to the bond's capital), A_m = f(S_m),
//!              available_m = floor(accrued_carve * A_m / sum A) - reserved_m (0 when sum A = 0)
//! ```
//!
//! Everything here is a pure function or a journaled write of [`PalwBondBudgetStateV1`]. The fold (`palw_bond_budget_fold_v1`, a child
//! of `palw_state_v2`) turns the journal into deltas 190/191; the state rides carriage tail `0xEF` and the root block `bond_budget/v1`,
//! both only when the engine exists — which nothing below the fence can cause. **No policy value is approved**: the fence carries a
//! versioned policy, and validation refuses arming either fence.

use borsh::{BorshDeserialize, BorshSerialize};
use std::collections::{BTreeMap, BTreeSet};

use crate::Hash64;
use crate::config::params::{ForkActivation, Params};
use crate::palw_mode_v2::PalwModeV2Error;
use crate::palw_state_v2::PalwBondKeyV2;

/// One physical reward block in the fixed-point block ledger (ADR-0176 D2: a block is never issued as `1/m`; its attribution is).
pub const PALW_BUDGET_BLOCK_UNIT_V1: u64 = 1_000_000;
/// The policy versions this binary reads.
/// The budget policy's version: 2 adds PESG §6's slots (2026-10-10) — the liability hold `H_L` apart from `W`, the external export cap
/// and the value-of-weight slot — and bounds `W` to ECON's derived range. Version 1 was never armed and is refused.
pub const PALW_BOND_BUDGET_POLICY_VERSION_V2: u16 = 2;

/// **PESG §6 / ECON §5e.4: the range a single-window (V2) `W` must stay in** (DAA): below `L = 280` claims outlive the window; above
/// 436 the 65,536-per-span Round bound stops the shared window from saturating (interim terms, 150 s/DAA).
///
/// **SUPERSEDED for any armable policy (ユーザー方針 2026-10-15, user decision 2026-10-14):** the common hold is 1-day-equivalent DAA
/// for Panel=1 and 7-day-equivalent DAA for Panel=0 ([`PALW_BOND_HOLD_PANEL_ONE_DAYS_V1`], [`PALW_BOND_HOLD_PANEL_ZERO_DAYS_V1`],
/// [`PALW_BOND_HOLD_DAA_PER_DAY_V1`]) carried by a V3 policy ([`PalwBondHoldPolicyV1`]). This range now bounds only the V2 mechanics
/// probe of the isolated calibration and the test harnesses; validation refuses a V2 policy anywhere else. The drill-interim
/// `W = 400` of readiness §3i is likewise superseded.
pub const PALW_BOND_BUDGET_W_MIN_DAA_V1: u64 = 280;
pub const PALW_BOND_BUDGET_W_MAX_DAA_V1: u64 = 436;

/// **ECON §5e.4: the interim liability hold `H_L`** (DAA), kept apart from the issuance clock `d + W`.
pub const PALW_BOND_BUDGET_LIABILITY_HOLD_INTERIM_DAA_V1: u64 = 280;

/// **PESG §6 / ECON §5e.4: the external export cap's ceiling**, permille of the collateral still held for the claim's liability: 0.51
/// (the share a conviction collects, `1 − a` with the 49% return).
pub const PALW_BOND_BUDGET_EXPORT_CAP_MAX_PERMILLE_V1: u16 = 510;
/// The allocation policy's version: 2 is the power curve `A_m = S_m^α` (ADR-0177's revised goal, 2026-10-10). Version 1, the piecewise
/// linear `f` of the "favour publication" goal, was never armed anywhere and is refused.
pub const PALW_MODEL_ALLOCATION_POLICY_VERSION_V2: u16 = 2;
/// The engine state's own version (its header).
pub const PALW_BOND_BUDGET_STATE_VERSION_V1: u16 = 1;
/// Dormant monetary extension: authenticated reporter binding / legacy quarantine.
pub const PALW_BOND_BUDGET_STATE_VERSION_V2: u16 = 2;
/// The V2 carriage tail of the engine (the Lead's allocation; present only when the engine exists).
pub const PALW_CARRIAGE_BOND_BUDGET_TAIL_V1: u8 = 0xEF;
/// The object tag of a bond's signed capital assignment (the Lead's allocation 140–149).
pub const PALW_CAPITAL_ASSIGNMENT_TAG_V1: u8 = 140;
/// The ML-DSA-87 context of a capital assignment's signature.
pub const PALW_CAPITAL_ASSIGNMENT_MLDSA87_CONTEXT_V1: &[u8] = b"misaka-palw/capital-assignment/object/v1";
const PALW_CAPITAL_ASSIGNMENT_MESSAGE_DOMAIN_V1: &[u8] = b"misaka-palw/capital-assignment/message/v1";
const PALW_BOND_BUDGET_POLICY_DOMAIN_V1: &[u8] = b"misaka-palw/bond-budget/policy/v1";
const PALW_BOND_BUDGET_HEADER_DOMAIN_V1: &[u8] = b"misaka-palw/bond-budget/header/v1";
const PALW_ROUND_RIGHTS_ROW_DOMAIN_V1: &[u8] = b"misaka-palw/bond-budget/round-rights-row/v1";
/// The largest exponent the curve takes, in half steps: `α ≤ 4` (`α` itself is POLICY). With `S_m` held to 64 bits, `S_m^(2α) < 2^512`
/// is computed exactly before its integer square root, and `A_m = ⌊S_m^α⌋ < 2^256`, so every weight, their sum over any number of models
/// a state can hold and `accrued · A_m` fit [`PalwAllocationWeightV1`] (512 bits) exactly.
pub const PALW_ALLOCATION_ALPHA_HALVES_MAX_V1: u8 = 8;

/// **The interim, UNAPPROVED exponent** the user picked (ECON round 3, 2026-10-10): `α = 1.5`, in half steps. Interim only — it is not
/// an approved activation value, and the fence stays refused.
pub const PALW_ALLOCATION_ALPHA_HALVES_INTERIM_V1: u8 = 3;

/// A model's allocation weight `A_m` and their sum `Σ A` — exact 512-bit integers.
pub type PalwAllocationWeightV1 = kaspa_math::Uint512;

/// The engine's tables (delta 190 names them; a table added later takes an id, not a delta number).
pub const PALW_BUDGET_TABLE_BONDS_V1: u8 = 1;
pub const PALW_BUDGET_TABLE_CLAIMS_V1: u8 = 2;
pub const PALW_BUDGET_TABLE_RELEASES_V1: u8 = 3;
pub const PALW_BUDGET_TABLE_ASSIGNMENTS_V1: u8 = 4;
pub const PALW_BUDGET_TABLE_MODELS_V1: u8 = 5;
/// PESG §6: the liability holds' expiries, `(hold_until, bond)`.
pub const PALW_BUDGET_TABLE_LIABILITY_RELEASES_V1: u8 = 6;
/// codex RFC-8 reporter income (monetary state v2). codex numbered it 6 — PESG §6's liability releases on the integration line —
/// so it moved to 9 at the R6781-F merge (7 / 8 stay codex's material income and consumer contracts).
pub const PALW_BUDGET_TABLE_REPORTER_INCOME_V1: u8 = 9;

mod consumer;
pub use consumer::*;

mod hold;
pub use hold::*;

mod allocation_v3;
pub use allocation_v3::*;

mod material;
pub use material::{
    PALW_BOND_BUDGET_STATE_VERSION_V3, PALW_BUDGET_TABLE_MATERIAL_INCOME_V1, PalwMaterialIncomeV1, PalwMaterialPaymentV1,
    material_pending_id_v1, material_queue_receipt_id_v1,
};

// ---- the four dimensions ------------------------------------------------------------------------------------------------------

/// A dimension of the budget (ADR-0176 D1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwBudgetDimV1 {
    /// `Q`: claims.
    Claims = 0,
    /// `B`: reward blocks, in [`PALW_BUDGET_BLOCK_UNIT_V1`] per block.
    BlockUnits = 1,
    /// `R`: reward, base units.
    Reward = 2,
    /// `F`: Final weight, the state's weight units.
    FinalWeight = 3,
    /// `T`: Round rights — execution tickets (readiness §3e; `PalwRoundRightsPolicyV1`).
    RoundRights = 4,
}

/// A `(Q, B, R, F)` vector: a cap, a window, a reservation or a consumption.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwBudgetVectorV1 {
    pub claims: u64,
    pub block_units: u64,
    pub reward_sompi: u64,
    pub final_weight: u128,
    /// Round rights (execution tickets) — readiness §3e.
    pub round_rights: u64,
}

impl PalwBudgetVectorV1 {
    pub const ZERO: Self = Self { claims: 0, block_units: 0, reward_sompi: 0, final_weight: 0, round_rights: 0 };

    pub fn is_zero(&self) -> bool {
        *self == Self::ZERO
    }

    pub fn get(&self, dim: PalwBudgetDimV1) -> u128 {
        match dim {
            PalwBudgetDimV1::Claims => self.claims as u128,
            PalwBudgetDimV1::BlockUnits => self.block_units as u128,
            PalwBudgetDimV1::Reward => self.reward_sompi as u128,
            PalwBudgetDimV1::FinalWeight => self.final_weight,
            PalwBudgetDimV1::RoundRights => self.round_rights as u128,
        }
    }

    pub fn checked_add(self, o: Self) -> Option<Self> {
        Some(Self {
            claims: self.claims.checked_add(o.claims)?,
            block_units: self.block_units.checked_add(o.block_units)?,
            reward_sompi: self.reward_sompi.checked_add(o.reward_sompi)?,
            final_weight: self.final_weight.checked_add(o.final_weight)?,
            round_rights: self.round_rights.checked_add(o.round_rights)?,
        })
    }

    pub fn checked_sub(self, o: Self) -> Option<Self> {
        Some(Self {
            claims: self.claims.checked_sub(o.claims)?,
            block_units: self.block_units.checked_sub(o.block_units)?,
            reward_sompi: self.reward_sompi.checked_sub(o.reward_sompi)?,
            final_weight: self.final_weight.checked_sub(o.final_weight)?,
            round_rights: self.round_rights.checked_sub(o.round_rights)?,
        })
    }

    /// Component-wise `≤`.
    pub fn fits_within(&self, caps: &Self) -> bool {
        self.first_excess(caps).is_none()
    }

    /// The first dimension (in `Q, B, R, F` order) where `self` exceeds `caps`.
    pub fn first_excess(&self, caps: &Self) -> Option<PalwBudgetDimV1> {
        [
            PalwBudgetDimV1::Claims,
            PalwBudgetDimV1::BlockUnits,
            PalwBudgetDimV1::Reward,
            PalwBudgetDimV1::FinalWeight,
            PalwBudgetDimV1::RoundRights,
        ]
        .into_iter()
        .find(|dim| self.get(*dim) > caps.get(*dim))
    }

    /// Component-wise minimum.
    pub fn min(self, o: Self) -> Self {
        Self {
            claims: self.claims.min(o.claims),
            block_units: self.block_units.min(o.block_units),
            reward_sompi: self.reward_sompi.min(o.reward_sompi),
            final_weight: self.final_weight.min(o.final_weight),
            round_rights: self.round_rights.min(o.round_rights),
        }
    }
}

/// **What a claim can at most derive** — computed by the writer of its path (design §2.1): an Attempt lead one block, its escrow and
/// its Final contribution; a rider no block of its own; a free-prompt claim `quanta` blocks, `quanta` carves and `quanta` quanta of
/// weight.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwBudgetAskV1 {
    pub block_units: u64,
    pub reward_sompi: u64,
    pub final_weight: u128,
    /// PESG §6: the collateral the claim reserves for its liability (`K`, the V2 claim's `reserved`). Open claims of one bond hold
    /// at most its capital in total (`Σ K ≤ C`, so at most `⌊C/K⌋` open claims of reservation `K`). 0 for a row that is no claim.
    pub liability_sompi: u128,
}

/// Where a reservation came from (recorded; `Legacy` rows are never consumed — old claims are paid under the rules they were
/// accepted under).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwBudgetOriginV1 {
    Attempt = 0,
    Rider = 1,
    FreePrompt = 2,
    /// A claim accepted below the fence, seeded into the window by the fence's first block (design §2.9).
    Legacy = 3,
    /// A kernel-route claim (hook H-1).
    KernelRoute = 4,
    /// A bond's Round rights drawn in one span (readiness §3e): reserved and used at the draw, released at `draw + W`.
    RoundRights = 5,
    /// Actual registrant/contributor market fee income; no claim/block/weight credit.
    MarketFee = 6,
    /// Fee-only execution Round income; no reward-bearing block credit.
    RoundFee = 7,
    /// Paid beacon-source fee promised before work, held through its signed liability and payment window.
    SourceFee = 8,
    /// Actual native Panel/EXEC income, held before work and through payment.
    NativeIncome = 9,
    /// Irrevocable native queue credit awaiting its actual coinbase payment.
    NativeIncomeQueued = 10,
    /// Unpaid remainder after one or more exact native queue credits; never transferable.
    NativeIncomePartial = 11,
    /// Immutable funded reporter entitlement, redeemed in exact recipient-R tranches.
    /// Outside the window until a tranche enters its own physical native queue.
    DeferredReporterIncome = 12,
    /// ユーザー方針 2026-10-15: a claim's shadow in its bond's **day view** (1-day-equivalent), under a synthetic bond key; never open,
    /// never live, never consumed, released at `accepted + W_day` ([`hold`]).
    HoldDay = 13,
    /// ユーザー方針 2026-10-15: a claim's shadow in its bond's **burst view** (released at `accepted + W_burst`).
    HoldBurst = 14,
    /// ユーザー判断 2026-10-15 (bond式の再設計, `palw_bond_formula_v2`): a terminal claim's liability kept in its bond's **liability
    /// view** until its liability ends (terminal + `H_L`, at least the court/DA/retention span) — open, never live, zero reservation;
    /// the concurrent-collateral check counts it.
    HoldLiability = 15,
}

/// **RFC-0016 P4 (user decision 2026-10-15): is this origin's reward user-paid escrow rather than a coinbase-derived leg?** A
/// kernel-route claim is paid out of its job's escrow, never issued (GAP-5), so the coinbase-derived model-allocation budget
/// (`model_available`, which accrues the worker carve) must never reduce it: the model clip applies only to coinbase-derived legs.
/// Such a claim still reserves its whole reward inside the producer's common `R` cap, and a cap that cannot hold it refuses the claim
/// before acceptance.
pub fn palw_budget_origin_is_user_escrow_v1(origin: PalwBudgetOriginV1) -> bool {
    matches!(origin, PalwBudgetOriginV1::KernelRoute)
}

/// Why the engine refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwBudgetRefusalV1 {
    /// The bond's window has no room in this dimension.
    Exhausted {
        dim: PalwBudgetDimV1,
    },
    /// The bond holds its policy's open-claim cap.
    OpenClaims,
    /// PESG §6: the bond's open claims would reserve more liability collateral than its capital (`Σ K > C`).
    LiabilityExceedsCapital,
    /// PESG §6: the bond's unsettled (pre-Final) weight would pass its Final-weight cap `F_max`.
    UnsettledWeight,
    /// The claim already holds a reservation.
    Duplicate,
    /// No such reservation, or it is closed.
    NotOpen,
    /// A strict consumption (a reward block) found less than it needs.
    Short {
        dim: PalwBudgetDimV1,
    },
    /// A shrink would grow a component, drop below what was consumed, or ran in another block than the acceptance.
    BadShrink,
    /// A capital assignment broke a rule (the reason).
    Assignment(&'static str),
    Overflow,
    /// ユーザー方針 2026-10-15: the bond's burst view (an additional short-period limit) has no room in this dimension.
    Burst {
        dim: PalwBudgetDimV1,
    },
    /// ADR-0177 (2026-10-14): the claim's model has no reward-bearing block opportunity left this epoch (`P_m` applied to blocks).
    ModelBlockOpportunity,
    /// **RFC-0016 P4 (user decision 2026-10-15):** a user-paid (escrow-funded) claim whose full escrow reward cannot be reserved — the
    /// policy's per-claim ceiling is below it. Decided BEFORE acceptance; an escrow reward is never clipped after.
    EscrowNotReservable,
}

impl std::fmt::Display for PalwBudgetRefusalV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Exhausted { dim } => write!(f, "the bond's budget window has no room in {dim:?} (ADR-0176 D1)"),
            Self::OpenClaims => write!(f, "the bond holds its open-claim cap (ADR-0176 D1)"),
            Self::LiabilityExceedsCapital => {
                write!(f, "the bond's open claims would reserve more liability collateral than its capital (PESG §6: open <= C/K)")
            }
            Self::UnsettledWeight => write!(f, "the bond's unsettled pre-Final weight would pass its Final-weight cap (PESG §6)"),
            Self::Duplicate => write!(f, "the claim already holds a budget reservation"),
            Self::NotOpen => write!(f, "the claim holds no open budget reservation"),
            Self::Short { dim } => write!(f, "the claim's reservation is short in {dim:?} (ADR-0176 D3)"),
            Self::BadShrink => write!(f, "a reservation may only shrink, in the block that took it, and never below its consumption"),
            Self::Assignment(why) => write!(f, "capital assignment refused: {why} (ADR-0177 D3)"),
            Self::Overflow => write!(f, "budget arithmetic overflow"),
            Self::Burst { dim } => {
                write!(f, "the bond's short-period burst cap has no room in {dim:?} (ADR-0176, ユーザー方針 2026-10-15)")
            }
            Self::ModelBlockOpportunity => {
                write!(f, "the model's block opportunity P_m is used up this allocation epoch (ADR-0177, RFC-0015 §8.5.5)")
            }
            Self::EscrowNotReservable => {
                write!(f, "the user-paid escrow reward exceeds one claim's reservation; refused, never clipped (RFC-0016 P4)")
            }
        }
    }
}

// ---- the policies and the fences ---------------------------------------------------------------------------------------------

/// **`palw_bond_budget_v1`'s policy (ADR-0176) — every value POLICY, none approved.** Rates are quoted per `capital_unit_sompi` of locked
/// capital per window `W`; `rho` multiplies `Q` alone (D1: raising claim capacity never raises `B`, `R` or `F`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwBondBudgetPolicyV1 {
    pub version: u16,
    /// `W`: the common window, DAA (`reuse_not_before = accepted_daa + W`).
    pub window_daa: u64,
    /// `u`: the capital quantum the rates are quoted against.
    pub capital_unit_sompi: u64,
    /// `ρ`: the common claim-capacity multiplier, ≥ 1.
    pub rho: u32,
    /// `q`: claims per `u` per `W` at `ρ = 1`.
    pub claims_per_unit: u64,
    /// `b`: reward-block units per `u` per `W` (`PALW_BUDGET_BLOCK_UNIT_V1` = one block).
    pub block_units_per_unit: u64,
    /// `r`: reward (base units) per `u` per `W`.
    pub reward_per_unit_sompi: u64,
    /// `w`: Final weight per `u` per `W`.
    pub final_weight_per_unit: u64,
    /// The separate outstanding-claim cap of one bond (RFC-0015 §8.3.1).
    pub max_open_claims_per_bond: u32,
    /// D2's "A → A/m": each claim's `R` and `F` are also capped at `⌊rate / (q·ρ)⌋` (POLICY; `B` is physical and never sliced).
    pub slice_rights_by_rho: bool,
    /// Readiness §3e: how Round rights (execution tickets, fee-only Rounds included) are bounded — POLICY P-9.
    pub round_rights: PalwRoundRightsPolicyV1,
    /// PESG §6 / ECON §5e.4: `H_L`, how long a bond's capital stays held after a claim is terminal (its liability), DAA — a clock of its
    /// own, apart from the issuance clock `d + W` (interim 280).
    pub liability_hold_daa: u64,
    /// PESG §6: the most of a claim's reward that may leave the chain's own books (into the payout queue that mints, or a market
    /// reserve) while its liability still holds collateral, permille of that collateral; at most 510 (0.51).
    pub export_cap_permille: u16,
    /// PESG §6: the value of Final weight — UNKNOWN today (FINX, BUDGET), so the slot is versioned and `Unknown` by default.
    pub weight_value: PalwWeightValuePolicyV1,
    /// **ユーザー方針 2026-10-15: the mode-aware common hold** — `Some` exactly for a V3 policy (whose per-unit rates are then per DAY
    /// and whose `window_daa` is Panel=0's 7-day span); `None` for V2. Serialized only for V3, so every V2 policy's bytes, digest and
    /// identity are unchanged.
    pub hold: Option<PalwBondHoldPolicyV1>,
}

impl BorshSerialize for PalwBondBudgetPolicyV1 {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        self.version.serialize(writer)?;
        self.window_daa.serialize(writer)?;
        self.capital_unit_sompi.serialize(writer)?;
        self.rho.serialize(writer)?;
        self.claims_per_unit.serialize(writer)?;
        self.block_units_per_unit.serialize(writer)?;
        self.reward_per_unit_sompi.serialize(writer)?;
        self.final_weight_per_unit.serialize(writer)?;
        self.max_open_claims_per_bond.serialize(writer)?;
        self.slice_rights_by_rho.serialize(writer)?;
        self.round_rights.serialize(writer)?;
        self.liability_hold_daa.serialize(writer)?;
        self.export_cap_permille.serialize(writer)?;
        self.weight_value.serialize(writer)?;
        // Some-only by version: a V2 policy writes exactly the bytes the derived encoding wrote.
        if self.version >= PALW_BOND_BUDGET_POLICY_VERSION_V3 {
            self.hold.serialize(writer)?;
        }
        Ok(())
    }
}

impl BorshDeserialize for PalwBondBudgetPolicyV1 {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version: u16 = BorshDeserialize::deserialize_reader(reader)?;
        Ok(Self {
            version,
            window_daa: BorshDeserialize::deserialize_reader(reader)?,
            capital_unit_sompi: BorshDeserialize::deserialize_reader(reader)?,
            rho: BorshDeserialize::deserialize_reader(reader)?,
            claims_per_unit: BorshDeserialize::deserialize_reader(reader)?,
            block_units_per_unit: BorshDeserialize::deserialize_reader(reader)?,
            reward_per_unit_sompi: BorshDeserialize::deserialize_reader(reader)?,
            final_weight_per_unit: BorshDeserialize::deserialize_reader(reader)?,
            max_open_claims_per_bond: BorshDeserialize::deserialize_reader(reader)?,
            slice_rights_by_rho: BorshDeserialize::deserialize_reader(reader)?,
            round_rights: BorshDeserialize::deserialize_reader(reader)?,
            liability_hold_daa: BorshDeserialize::deserialize_reader(reader)?,
            export_cap_permille: BorshDeserialize::deserialize_reader(reader)?,
            weight_value: BorshDeserialize::deserialize_reader(reader)?,
            hold: if version >= PALW_BOND_BUDGET_POLICY_VERSION_V3 { BorshDeserialize::deserialize_reader(reader)? } else { None },
        })
    }
}

/// **The value of a unit of Final weight** (PESG §6, ECON §5e.4: `w·v_F` belongs inside the same per-bond budget as `r`). The value is
/// UNKNOWN, so the slot ships `Unknown` and the engine bounds weight by `F_max` alone; `SompiPerWeight` makes admission also hold
/// `reward + final_weight · v ≤ R_max` per window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwWeightValuePolicyV1 {
    Unknown = 0,
    SompiPerWeight { sompi_per_weight: u64 } = 1,
}

/// **How a bond's Round rights are bounded** (readiness §3e, POLICY P-9). Both modes cap every ticket before the draw; neither leaves
/// a fee-only Round outside the budget.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwRoundRightsPolicyV1 {
    /// Each allocated ticket takes one reward block ([`PALW_BUDGET_BLOCK_UNIT_V1`]) of the bond's `B`.
    CountAgainstBlocks = 0,
    /// A separate dimension: `⌊C · rights_per_unit / u⌋` tickets per window.
    ExecutionCap { rights_per_unit: u64 } = 1,
}

impl PalwBondBudgetPolicyV1 {
    /// **NOT A PROPOSAL.** The smallest well-formed value, so a fence probe and the identity hashers have something to read. Every
    /// rate is one unit; no network could run on it, and none should read it as a recommendation.
    pub fn unapproved_probe_v1() -> Self {
        Self {
            version: PALW_BOND_BUDGET_POLICY_VERSION_V2,
            window_daa: PALW_BOND_BUDGET_W_MIN_DAA_V1,
            capital_unit_sompi: 1,
            rho: 1,
            claims_per_unit: 1,
            block_units_per_unit: 1,
            reward_per_unit_sompi: 1,
            final_weight_per_unit: 1,
            max_open_claims_per_bond: 1,
            slice_rights_by_rho: false,
            // Mode is the user's decision; this unapproved numeric probe still
            // cannot activate any network.
            round_rights: PalwRoundRightsPolicyV1::ExecutionCap { rights_per_unit: 1 },
            liability_hold_daa: PALW_BOND_BUDGET_LIABILITY_HOLD_INTERIM_DAA_V1,
            export_cap_permille: PALW_BOND_BUDGET_EXPORT_CAP_MAX_PERMILLE_V1,
            weight_value: PalwWeightValuePolicyV1::Unknown,
            hold: None,
        }
    }

    /// **Mechanics-only policy for networks that arm `palw_panel_free_v1` without a real activation** (isolated calibration, the
    /// private integration profiles, test-armed worlds; RFC-0015 §8.3 lead default 2026-10-15: every Panel-free claim is budgeted, so
    /// these networks carry a budget too). UNAPPROVED and never a proposal: one-KAS capital unit, `W` at ECON's lower bound, ample
    /// claims / open claims / Round rights, reward and Final weight one unit per unit of capital per `W`.
    pub fn opv_mechanics_probe_v1() -> Self {
        let mut p = Self::unapproved_probe_v1();
        p.capital_unit_sompi = crate::constants::SOMPI_PER_KASPA;
        p.claims_per_unit = 1_000;
        p.block_units_per_unit = PALW_BUDGET_BLOCK_UNIT_V1;
        p.reward_per_unit_sompi = crate::constants::SOMPI_PER_KASPA;
        p.final_weight_per_unit = 1_000;
        p.max_open_claims_per_bond = 1_000;
        p.round_rights = PalwRoundRightsPolicyV1::ExecutionCap { rights_per_unit: 1_000 };
        p
    }

    /// The value's own refusals: the version, every rate and quantum positive, `ρ ≥ 1`, and `q·ρ` fitting a `u64` (so every cap is an
    /// exact `u128` product).
    pub fn validate(&self) -> Result<(), &'static str> {
        match (self.version, self.hold.as_ref()) {
            (PALW_BOND_BUDGET_POLICY_VERSION_V2, None) => {
                if !(PALW_BOND_BUDGET_W_MIN_DAA_V1..=PALW_BOND_BUDGET_W_MAX_DAA_V1).contains(&self.window_daa) {
                    return Err(
                        "a single-window (V2) W must be in ECON's derived range 280..=436 DAA (PESG §6; V2 is mechanics-only)",
                    );
                }
            }
            (PALW_BOND_BUDGET_POLICY_VERSION_V3, Some(hold)) => {
                hold.validate()?;
                if self.window_daa != hold.window_daa(PalwHoldModeV1::PanelZero) {
                    return Err("a V3 policy's window W is Panel=0's 7-day-equivalent span (panel_zero_days · daa_per_day)");
                }
            }
            (PALW_BOND_BUDGET_POLICY_VERSION_V2 | PALW_BOND_BUDGET_POLICY_VERSION_V3, _) => {
                return Err("a V3 policy carries the hold policy and a V2 policy carries none");
            }
            _ => return Err("the bond budget policy's version is not one this binary reads"),
        }
        if self.liability_hold_daa == 0 {
            return Err("the liability hold H_L must be positive");
        }
        if self.export_cap_permille > PALW_BOND_BUDGET_EXPORT_CAP_MAX_PERMILLE_V1 {
            return Err("the export cap is at most 0.51 of the collateral still held (PESG §6)");
        }
        if self.weight_value == (PalwWeightValuePolicyV1::SompiPerWeight { sompi_per_weight: 0 }) {
            return Err("a known value of weight must be positive");
        }
        if self.capital_unit_sompi == 0 {
            return Err("the capital unit must be positive");
        }
        if self.rho == 0 {
            return Err("rho must be at least one");
        }
        if self.claims_per_unit == 0
            || self.block_units_per_unit == 0
            || self.reward_per_unit_sompi == 0
            || self.final_weight_per_unit == 0
        {
            return Err("every per-unit rate must be positive");
        }
        if self.claims_per_unit.checked_mul(self.rho as u64).is_none() {
            return Err("claims_per_unit · rho must fit a u64");
        }
        if self.max_open_claims_per_bond == 0 {
            return Err("the open-claim cap must be positive");
        }
        if self.round_rights == (PalwRoundRightsPolicyV1::ExecutionCap { rights_per_unit: 0 }) {
            return Err("an execution cap must allow a positive number of Round rights per unit");
        }
        Ok(())
    }

    /// `q · ρ` (claims per unit at this ρ).
    pub fn claims_per_unit_at_rho(&self) -> u64 {
        self.claims_per_unit.saturating_mul(self.rho as u64)
    }

    /// The policy's digest (the engine header records the one it was created under).
    pub fn digest(&self) -> Hash64 {
        keyed64(PALW_BOND_BUDGET_POLICY_DOMAIN_V1, &[&borsh::to_vec(self).expect("a policy serializes")])
    }
}

/// **`Params::palw_bond_budget_v1`'s value**: the activation and the policy. Some-only everywhere: `None` on every preset, hashed into both
/// fingerprints only when `Some` (value included), collapsed whole from `Some(never())`, its activation alone visited by `for_each_fence`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwBondBudgetFenceV1 {
    pub activation: ForkActivation,
    pub policy: PalwBondBudgetPolicyV1,
}

impl PalwBondBudgetFenceV1 {
    /// A bare height with the [`PalwBondBudgetPolicyV1::unapproved_probe_v1`] value (what a fence probe builds from a height alone).
    pub fn at(activation: ForkActivation) -> Self {
        Self { activation, policy: PalwBondBudgetPolicyV1::unapproved_probe_v1() }
    }

    /// The value as the identity hashers write it, after the activation.
    pub(crate) fn write_value_into(&self, h: &mut kaspa_hashes::ConsensusParamsId) {
        h.write(borsh::to_vec(&self.policy).expect("the policy serializes"));
    }
}

/// **The allocation curve (ADR-0177 D4 as revised 2026-10-10; POLICY)**: `A_m = ⌊S_m^α⌋`, `S_m` in base units of capital, `α` in half
/// steps (`alpha_halves = 2α`: 2 is linear, 3 is the interim 1.5, 4 is the comparison value 2). `α = 1` is neutral to splitting and
/// merging; `α > 1` strongly favours a model with more effective locked bond — a fixed budget's share, never more issuance
/// (`R_m = ⌊R_PALW · A_m / Σ_j A_j⌋`). No saturation and no per-model cap: the bonds' own Q/B/R/F caps bound what a claim is paid.
///
/// **Rounding (exact and deterministic).** `A_m = isqrt(S_m^(2α))`: the power `S_m^(2α)` is an exact integer (`2α` is), and
/// `isqrt(n) = max { r : r² ≤ n }` is the floor of the real `S_m^α` — for an integer `α` the root of a perfect square, so `S_m^α` exactly.
/// Then `R_m = ⌊R_PALW · A_m / ΣA⌋`, the one division, floored. **Proof that `Σ_m R_m ≤ R_PALW`:** `Σ_m ⌊R·A_m/ΣA⌋ ≤ Σ_m R·A_m/ΣA = R`.
/// The remainder `R − Σ R_m` is below the number of models, is allocated to nobody and is never minted (P-7). Flooring the weights
/// keeps the split property: `⌊x^α⌋ + ⌊y^α⌋ ≤ ⌊x^α + y^α⌋ ≤ ⌊(x+y)^α⌋` for `α ≥ 1`, so two models never out-weigh one with their capital.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwAllocationCurveV1 {
    /// `2α`, in `2..=PALW_ALLOCATION_ALPHA_HALVES_MAX_V1` (`α ∈ {1, 1.5, 2, …, 4}`).
    pub alpha_halves: u8,
}

impl PalwAllocationCurveV1 {
    /// The interim, unapproved `α = 1.5`.
    pub const fn interim_unapproved_v1() -> Self {
        Self { alpha_halves: PALW_ALLOCATION_ALPHA_HALVES_INTERIM_V1 }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.alpha_halves < 2 || self.alpha_halves > PALW_ALLOCATION_ALPHA_HALVES_MAX_V1 {
            return Err("the allocation exponent must be alpha = alpha_halves / 2 in 1..=4 (alpha_halves in 2..=8)");
        }
        Ok(())
    }

    /// `A_m = ⌊S_m^α⌋ = isqrt(S_m^(2α))`, exact. `S_m` is held to 64 bits first: no state can lock more than `u64::MAX` base units (the
    /// supply is below it), so the clamp is unreachable and only keeps the 512-bit bound a property of the code.
    pub fn weight(&self, capital: u128) -> PalwAllocationWeightV1 {
        let s = u64::try_from(capital).unwrap_or(u64::MAX);
        let mut power = PalwAllocationWeightV1::from_u64(1);
        for _ in 0..self.alpha_halves.min(PALW_ALLOCATION_ALPHA_HALVES_MAX_V1) {
            let (next, overflow) = power.overflowing_mul_u64(s);
            debug_assert!(!overflow, "S < 2^64 and 2 alpha <= 8 keep S^(2 alpha) below 2^512");
            power = next;
        }
        palw_isqrt_u512_v1(power)
    }
}

/// **`isqrt(n) = max { r : r² ≤ n }`** for a 512-bit `n`, exactly (Newton's method from above: `x₀ ≥ √n`, `x ← ⌊(x + ⌊n/x⌋)/2⌋` while
/// it decreases; the first non-decrease is the floor root). Deterministic, no floating point.
pub fn palw_isqrt_u512_v1(n: PalwAllocationWeightV1) -> PalwAllocationWeightV1 {
    if n.is_zero() {
        return n;
    }
    // x₀ = 2^⌈bits/2⌉ > √n.
    let (mut x, _) = PalwAllocationWeightV1::from_u64(1).overflowing_shl(n.bits().div_ceil(2));
    loop {
        let (q, _) = n.div_rem(x);
        let (sum, _) = x.overflowing_add(q);
        let (y, _) = sum.overflowing_shr(1);
        if y >= x {
            return x;
        }
        x = y;
    }
}

/// **`palw_model_bond_allocation_v1`'s policy (ADR-0177 D3–D5) — every value POLICY, none approved.**
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwModelAllocationPolicyV1 {
    pub version: u16,
    /// `E`: the allocation epoch, DAA. Its tie to the budget's `W` is POLICY.
    pub epoch_daa: u64,
    /// Full epochs an increase waits before it counts (short-term borrowing; decreases apply at once).
    pub seasoning_epochs: u32,
    /// `A_m = S_m^α` (POLICY: `α`).
    pub curve: PalwAllocationCurveV1,
    /// Entries one assignment may name.
    pub max_models_per_bond: u16,
    /// **User decision 2026-10-14 (RFC-0015 §8.5.5)** — the 30-day average of miner + claim-bound verifier capital, `P_m` applied to
    /// block opportunity and coinbase ([`allocation_v3`]). `Some` exactly for a V3 policy; serialized only for V3, so every V2
    /// policy's bytes, digest and identity are unchanged.
    pub average: Option<PalwAllocationAverageV1>,
}

impl BorshSerialize for PalwModelAllocationPolicyV1 {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        self.version.serialize(writer)?;
        self.epoch_daa.serialize(writer)?;
        self.seasoning_epochs.serialize(writer)?;
        self.curve.serialize(writer)?;
        self.max_models_per_bond.serialize(writer)?;
        if self.version >= PALW_MODEL_ALLOCATION_POLICY_VERSION_V3 {
            self.average.serialize(writer)?;
        }
        Ok(())
    }
}

impl BorshDeserialize for PalwModelAllocationPolicyV1 {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version: u16 = BorshDeserialize::deserialize_reader(reader)?;
        Ok(Self {
            version,
            epoch_daa: BorshDeserialize::deserialize_reader(reader)?,
            seasoning_epochs: BorshDeserialize::deserialize_reader(reader)?,
            curve: BorshDeserialize::deserialize_reader(reader)?,
            max_models_per_bond: BorshDeserialize::deserialize_reader(reader)?,
            average: if version >= PALW_MODEL_ALLOCATION_POLICY_VERSION_V3 {
                BorshDeserialize::deserialize_reader(reader)?
            } else {
                None
            },
        })
    }
}

impl PalwModelAllocationPolicyV1 {
    /// **NOT A PROPOSAL** (as [`PalwBondBudgetPolicyV1::unapproved_probe_v1`]).
    pub fn unapproved_probe_v1() -> Self {
        Self {
            version: PALW_MODEL_ALLOCATION_POLICY_VERSION_V2,
            epoch_daa: 1,
            seasoning_epochs: 1,
            curve: PalwAllocationCurveV1::interim_unapproved_v1(),
            max_models_per_bond: 1,
            average: None,
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        match (self.version, self.average.is_some()) {
            (PALW_MODEL_ALLOCATION_POLICY_VERSION_V2, false) => {}
            (PALW_MODEL_ALLOCATION_POLICY_VERSION_V3, true) => self.validate_v3()?,
            (PALW_MODEL_ALLOCATION_POLICY_VERSION_V2 | PALW_MODEL_ALLOCATION_POLICY_VERSION_V3, _) => {
                return Err("a V3 allocation policy carries the averaging terms and a V2 policy carries none");
            }
            _ => return Err("the allocation policy's version is not one this binary reads"),
        }
        if self.epoch_daa == 0 {
            return Err("the allocation epoch must be positive");
        }
        if self.seasoning_epochs == 0 {
            return Err("an increase must season at least one full epoch");
        }
        if self.max_models_per_bond == 0 {
            return Err("an assignment must be able to name a model");
        }
        self.curve.validate()
    }
}

/// **`Params::palw_model_bond_allocation_v1`'s value** — Some-only, as [`PalwBondBudgetFenceV1`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwModelBondAllocationFenceV1 {
    pub activation: ForkActivation,
    pub policy: PalwModelAllocationPolicyV1,
}

impl PalwModelBondAllocationFenceV1 {
    pub fn at(activation: ForkActivation) -> Self {
        Self { activation, policy: PalwModelAllocationPolicyV1::unapproved_probe_v1() }
    }

    pub(crate) fn write_value_into(&self, h: &mut kaspa_hashes::ConsensusParamsId) {
        h.write(borsh::to_vec(&self.policy).expect("the policy serializes"));
    }
}

/// **The fences, mirrored on the V2 bundle's state params** (`PalwStateParamsV2::bond_budget`), which the fold reads. Not Borsh and not
/// hashed — the fences themselves are what the identity ids name (Some-only), and validation refuses both armed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwBondBudgetMirrorV1 {
    pub from_daa: u64,
    pub policy: PalwBondBudgetPolicyV1,
    /// ADR-0177: `(from_daa, policy)` of the allocation fence, when armed too.
    pub allocation: Option<(u64, PalwModelAllocationPolicyV1)>,
}

impl PalwBondBudgetMirrorV1 {
    pub fn active_at(&self, daa: u64) -> bool {
        daa >= self.from_daa
    }

    pub fn allocation_at(&self, daa: u64) -> Option<&PalwModelAllocationPolicyV1> {
        self.allocation.as_ref().filter(|(from, _)| daa >= *from).map(|(_, p)| p)
    }

    /// The allocation epoch `daa` falls in (`None` below the allocation fence).
    pub fn allocation_epoch_at(&self, daa: u64) -> Option<u64> {
        let (from, policy) = self.allocation.as_ref()?;
        (daa >= *from).then(|| (daa - from) / policy.epoch_daa.max(1))
    }
}

impl Params {
    /// Whether the bond budget is in force at `daa_score` — never, in this binary.
    pub fn palw_bond_budget_active_at(&self, daa_score: u64) -> bool {
        self.palw_bond_budget_v1.as_ref().is_some_and(|f| f.activation != ForkActivation::never() && f.activation.is_active(daa_score))
    }

    pub fn palw_reporter_income_active_at(&self, daa: u64) -> bool {
        self.palw_bond_budget_active_at(daa)
            && self.palw_reporter_income_v2.is_some_and(|at| at != ForkActivation::never() && at.is_active(daa))
    }

    pub fn validate_palw_reporter_income_v2(&self) -> Result<(), PalwModeV2Error> {
        let Some(at) = self.palw_reporter_income_v2.filter(|at| *at != ForkActivation::never()) else { return Ok(()) };
        if !self
            .palw_bond_budget_v1
            .as_ref()
            .is_some_and(|f| f.activation != ForkActivation::never() && f.activation.daa_score() <= at.daa_score())
        {
            return Err(PalwModeV2Error::Invalid("palw_reporter_income_v2 requires palw_bond_budget_v1 at or below it"));
        }
        Err(PalwModeV2Error::Invalid(
            "palw_reporter_income_v2 cannot be armed: monetary schema v2 and legacy cash migration are dormant pending complete activation validation",
        ))
    }

    pub fn palw_material_income_active_at(&self, daa: u64) -> bool {
        self.palw_reporter_income_active_at(daa)
            && self.palw_material_income_v3.is_some_and(|at| at != ForkActivation::never() && at.is_active(daa))
    }

    pub fn validate_palw_material_income_v3(&self) -> Result<(), PalwModeV2Error> {
        let Some(at) = self.palw_material_income_v3.filter(|at| *at != ForkActivation::never()) else { return Ok(()) };
        if !self
            .palw_bond_budget_v1
            .as_ref()
            .is_some_and(|f| f.activation != ForkActivation::never() && f.activation.daa_score() <= at.daa_score())
            || !self.palw_reporter_income_v2.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= at.daa_score())
        {
            return Err(PalwModeV2Error::Invalid("palw_material_income_v3 requires budget and reporter schema at or below it"));
        }
        Err(PalwModeV2Error::Invalid(
            "palw_material_income_v3 cannot be armed: complete signed material/TTL and all consumer lifecycle validation remain pending",
        ))
    }

    /// Whether the model-bond allocation is in force at `daa_score` — never, in this binary (it needs the budget too).
    pub fn palw_model_bond_allocation_active_at(&self, daa_score: u64) -> bool {
        self.palw_bond_budget_active_at(daa_score)
            && self
                .palw_model_bond_allocation_v1
                .as_ref()
                .is_some_and(|f| f.activation != ForkActivation::never() && f.activation.is_active(daa_score))
    }

    /// **The budget fence's refusal**: a malformed policy is named first (never hidden behind the arming refusal), then any armed height
    /// is refused — ADR-0176's values (W, the ρ mapping, the caps) are POLICY the user has not set, and the reward/weight fences the budget
    /// bounds are not armable.
    pub fn validate_palw_bond_budget_v1(&self) -> Result<(), PalwModeV2Error> {
        let Some(fence) = self.palw_bond_budget_v1.as_ref().filter(|f| f.activation != ForkActivation::never()) else { return Ok(()) };
        if fence.policy.validate().is_err() {
            return Err(PalwModeV2Error::Invalid(
                "palw_bond_budget_v1's policy is invalid (PalwBondBudgetPolicyV1::validate names the rule that fails)",
            ));
        }
        if !matches!(fence.policy.round_rights, PalwRoundRightsPolicyV1::ExecutionCap { .. }) {
            return Err(PalwModeV2Error::Invalid(
                "fee-only Rounds require a dedicated execution cap; B_max is for reward-bearing blocks",
            ));
        }
        // RFC-0015 §8.3 (lead default 2026-10-15, pending user review): an isolated calibration genesis carries the mechanics budget
        // (every Panel-free claim budgeted there too). Its two economic prerequisites below are NOT required there (calibration
        // deviation, recorded in docs/design/palw/rfc-0015-implementation-status-2026-10-15.md): `palw_audit_2026_09_23` is
        // genesis-only and `palw_economic_safety` brings the windowed mint, neither of which the calibration devnet base carries.
        if self.has_isolated_native_calibration_v34() {
            return if fence.activation == ForkActivation::always() && fence.policy == PalwBondBudgetPolicyV1::opv_mechanics_probe_v1()
            {
                Ok(())
            } else {
                Err(PalwModeV2Error::Invalid("an isolated calibration carries exactly the always-on OPV mechanics budget"))
            };
        }
        // ユーザー方針 2026-10-15: outside the isolated calibration only a V3 policy (the mode-aware 1-day / 7-day common hold with
        // per-day equalized caps, burst and concurrent-liability caps) is armable; a single-window V2 policy is mechanics-only.
        if fence.policy.version != PALW_BOND_BUDGET_POLICY_VERSION_V3 || fence.policy.hold.is_none() {
            return Err(PalwModeV2Error::Invalid(
                "palw_bond_budget_v1 needs a V3 policy: the common hold is 1-day-equivalent DAA (Panel=1) / 7-day-equivalent DAA (Panel=0) with per-day equalized caps (ユーザー方針 2026-10-15)",
            ));
        }
        // A budget-refused own attempt is SKIPPED, and only past the 2026-09-23 audit fence is a skipped attempt's carve withheld
        // (`palw_v2_skipped_own_attempt_carve`); below it the block's whole worker carve would be paid for no claim.
        let audit_below =
            self.palw_audit_2026_09_23.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= fence.activation.daa_score());
        if !audit_below {
            return Err(PalwModeV2Error::Invalid(
                "palw_bond_budget_v1 requires palw_audit_2026_09_23 at or below it: a budget-refused attempt's carve is withheld only there",
            ));
        }
        // Readiness §3e: the Round rights are capped in the windowed mint, which is ADR-0151's (`palw_economic_safety`).
        let safety_below =
            self.palw_economic_safety.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= fence.activation.daa_score());
        if !safety_below {
            return Err(PalwModeV2Error::Invalid(
                "palw_bond_budget_v1 requires palw_economic_safety at or below it: Round rights are capped in its windowed mint",
            ));
        }
        Err(PalwModeV2Error::Invalid(
            "palw_bond_budget_v1 cannot be armed: ADR-0176's window W, the rho -> Q/B/R/F mapping and the cap values are POLICY the user has not set, and no reward or consensus-weight fence it bounds is armable",
        ))
    }

    /// **The allocation fence's refusal**: the policy checked first, the budget fence required at or below it (D5: model coinbase never
    /// raises a bond's own caps, so the caps must exist), then any armed height refused — `f`, the epoch and seasoning are POLICY and
    /// ECON's open-versus-closed evaluation (D6) does not exist.
    pub fn validate_palw_model_bond_allocation_v1(&self) -> Result<(), PalwModeV2Error> {
        let Some(fence) = self.palw_model_bond_allocation_v1.as_ref().filter(|f| f.activation != ForkActivation::never()) else {
            return Ok(());
        };
        if fence.policy.validate().is_err() {
            return Err(PalwModeV2Error::Invalid(
                "palw_model_bond_allocation_v1's policy is invalid (PalwModelAllocationPolicyV1::validate names the rule that fails)",
            ));
        }
        let budget_below = self
            .palw_bond_budget_v1
            .as_ref()
            .is_some_and(|b| b.activation != ForkActivation::never() && b.activation.daa_score() <= fence.activation.daa_score());
        if !budget_below {
            return Err(PalwModeV2Error::Invalid(
                "palw_model_bond_allocation_v1 requires palw_bond_budget_v1 at or below it (ADR-0177 D5)",
            ));
        }
        Err(PalwModeV2Error::Invalid(
            "palw_model_bond_allocation_v1 cannot be armed: the curve f, the allocation epoch and seasoning are POLICY the user has not set, and ADR-0177 D6's open-versus-closed evaluation does not exist",
        ))
    }

    /// **RFC-0015 §8.3 (lead default 2026-10-15): a network that declares the OPV policy budgets every Panel-free claim.** Where
    /// `palw_panel_free_v1` is armed and `palw_bond_budget_v1` is absent, arm the budget at the same height with
    /// [`PalwBondBudgetPolicyV1::opv_mechanics_probe_v1`] and re-mirror it. For isolated calibration, private profiles and test
    /// harnesses only: a real activation brings its reviewed policy (the activation gate binds its digest). Returns whether it armed.
    pub fn arm_palw_opv_mechanics_budget_v1(&mut self) -> bool {
        let Some(at) = self.palw_panel_free_v1.as_ref().map(|f| f.activation).filter(|a| *a != ForkActivation::never()) else {
            return false;
        };
        if self.palw_bond_budget_v1.is_some() {
            return false;
        }
        self.palw_bond_budget_v1 =
            Some(PalwBondBudgetFenceV1 { activation: at, policy: PalwBondBudgetPolicyV1::opv_mechanics_probe_v1() });
        self.sync_palw_bond_budget_v1();
        true
    }

    /// **The fences' mirror** on the V2 bundle's state params. `None` on every shipped preset; a fixture that bypasses validation calls
    /// this after arming the fences.
    pub fn sync_palw_bond_budget_v1(&mut self) {
        // ADR-0176 amendment: the bond formula's mirror rides every budget sync (fixtures arm both, then sync once).
        self.sync_palw_bond_formula_v2();
        let mirror =
            self.palw_bond_budget_v1.as_ref().filter(|f| f.activation != ForkActivation::never()).map(|f| PalwBondBudgetMirrorV1 {
                from_daa: f.activation.daa_score(),
                policy: f.policy.clone(),
                allocation: self
                    .palw_model_bond_allocation_v1
                    .as_ref()
                    .filter(|a| a.activation != ForkActivation::never())
                    .map(|a| (a.activation.daa_score().max(f.activation.daa_score()), a.policy.clone())),
            });
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle
                .state
                .clone()
                .with_bond_budget(mirror)
                .with_claim_challenge_v2(self.palw_claim_challenge_v2.filter(|f| f.activation != ForkActivation::never()))
                .with_reporter_income_from(
                    self.palw_reporter_income_v2.filter(|at| *at != ForkActivation::never()).map(|at| at.daa_score()),
                )
                .with_material_income_from(
                    self.palw_material_income_v3.filter(|at| *at != ForkActivation::never()).map(|at| at.daa_score()),
                )
                .with_material_release_recovery_v2(
                    self.palw_material_release_recovery_v2.as_ref().filter(|f| f.activation != ForkActivation::never()).map(|f| {
                        crate::palw_material_release_recovery_v2::PalwMaterialReleaseRecoveryMirrorV2 {
                            from_daa: f.activation.daa_score(),
                            policy: f.policy.clone(),
                        }
                    }),
                )
                .with_monetary_material(
                    self.palw_monetary_material_v1.as_ref().filter(|f| f.activation != ForkActivation::never()).map(|f| {
                        crate::palw_monetary_material_v1::PalwMonetaryMaterialMirrorV1 {
                            from_daa: f.activation.daa_score(),
                            policy: f.policy.clone(),
                        }
                    }),
                )
                .with_paid_consumer_lifecycle(
                    self.palw_paid_consumer_lifecycle_v1
                        .as_ref()
                        .filter(|f| f.activation != ForkActivation::never())
                        .map(|f| PalwPaidConsumerLifecycleMirrorV1 { from_daa: f.activation.daa_score(), policy: f.policy }),
                );
        }
    }
}

// ---- pure functions ----------------------------------------------------------------------------------------------------------

fn floor_mul_div(a: u128, b: u128, d: u128) -> u128 {
    // Every caller bounds `a, b ≤ u64::MAX`, so `a · b < 2^128` is exact; saturate rather than panic on an unvalidated input.
    a.checked_mul(b).map(|p| p / d.max(1)).unwrap_or(u128::MAX)
}

/// **The caps of a bond with locked capital `capital`** (design §2.3): `⌊C·q·ρ/u⌋`, `⌊C·b/u⌋`, `⌊C·r/u⌋`, `⌊C·w/u⌋` — linear in `C`
/// with floors, hence sub-additive over any split of `C`; `ρ` reaches `Q` only.
pub fn palw_bond_budget_caps_v1(policy: &PalwBondBudgetPolicyV1, capital: u64) -> PalwBudgetVectorV1 {
    // ユーザー方針 2026-10-15: under a V3 policy the window is Panel=0's 7-day span and its caps are 7 · daily(C) (the long view).
    if let Some(hold) = policy.hold.as_ref() {
        return palw_bond_hold_period_caps_v1(policy, hold, capital, PalwHoldModeV1::PanelZero);
    }
    let c = capital as u128;
    let u = policy.capital_unit_sompi as u128;
    let sat = |x: u128| u64::try_from(x).unwrap_or(u64::MAX);
    PalwBudgetVectorV1 {
        claims: sat(floor_mul_div(c, policy.claims_per_unit_at_rho() as u128, u)),
        block_units: sat(floor_mul_div(c, policy.block_units_per_unit as u128, u)),
        reward_sompi: sat(floor_mul_div(c, policy.reward_per_unit_sompi as u128, u)),
        final_weight: floor_mul_div(c, policy.final_weight_per_unit as u128, u),
        round_rights: match policy.round_rights {
            // Each ticket takes a whole block of B: at most B_max's whole blocks (B binds alongside, shared with claims).
            PalwRoundRightsPolicyV1::CountAgainstBlocks => {
                sat(floor_mul_div(c, policy.block_units_per_unit as u128, u)) / PALW_BUDGET_BLOCK_UNIT_V1
            }
            PalwRoundRightsPolicyV1::ExecutionCap { rights_per_unit } => sat(floor_mul_div(c, rights_per_unit as u128, u)),
        },
    }
}

/// **What `n` Round rights reserve** under `policy` (readiness §3e): `n` tickets, and `n` blocks where they count against `B`.
pub fn palw_round_rights_vector_v1(policy: &PalwBondBudgetPolicyV1, n: u64) -> PalwBudgetVectorV1 {
    let blocks = match policy.round_rights {
        PalwRoundRightsPolicyV1::CountAgainstBlocks => n.saturating_mul(PALW_BUDGET_BLOCK_UNIT_V1),
        PalwRoundRightsPolicyV1::ExecutionCap { .. } => 0,
    };
    PalwBudgetVectorV1 { round_rights: n, block_units: blocks, ..PalwBudgetVectorV1::ZERO }
}

/// **A bond's remaining Round rights just before a draw** (readiness §3e: `BondRemainingRoundRights`) — its caps at its capital less
/// every reservation still in its window (other spans' draws, claims' blocks), in the binding dimension of the policy's mode. The open-
/// claim cap does not apply (a draw opens no claim).
pub fn palw_round_rights_remaining_v1(policy: &PalwBondBudgetPolicyV1, capital: u64, row: Option<&PalwBudgetBondRowV1>) -> u64 {
    let caps = palw_bond_budget_caps_v1(policy, capital);
    let window = row.map(|r| r.window).unwrap_or_default();
    // Boundary migration can preserve accepted rights above a new cap. The
    // reservation checks the full vector, so its candidate set must do so too.
    if window.first_excess(&caps).is_some() {
        return 0;
    }
    match policy.round_rights {
        PalwRoundRightsPolicyV1::CountAgainstBlocks => (caps.block_units.saturating_sub(window.block_units)
            / PALW_BUDGET_BLOCK_UNIT_V1)
            .min(caps.round_rights.saturating_sub(window.round_rights)),
        PalwRoundRightsPolicyV1::ExecutionCap { .. } => caps.round_rights.saturating_sub(window.round_rights),
    }
}

/// The budget row of a bond's Round rights drawn in `span` (one per `(span, bond)`).
pub fn palw_round_rights_row_id_v1(span: u64, bond: &PalwBondKeyV2) -> Hash64 {
    keyed64(PALW_ROUND_RIGHTS_ROW_DOMAIN_V1, &[&span.to_le_bytes(), &borsh::to_vec(bond).expect("a bond key serializes")])
}

/// **The per-claim ceilings** when the policy slices rights by ρ: `R ≤ ⌊r/(q·ρ)⌋`, `F ≤ ⌊w/(q·ρ)⌋` (so `Q_max` such claims fit `R_max`
/// and `F_max`); `None` when it does not.
pub fn palw_bond_budget_claim_ceilings_v1(policy: &PalwBondBudgetPolicyV1) -> Option<(u64, u128)> {
    if !policy.slice_rights_by_rho {
        return None;
    }
    let qr = policy.claims_per_unit_at_rho().max(1);
    Some((policy.reward_per_unit_sompi / qr, (policy.final_weight_per_unit / qr) as u128))
}

/// **A claim's reservation from its ask** (design §2.5): one claim, the ask's blocks (never sliced: blocks are physical), and its reward
/// and weight clipped by the per-claim ceilings where the policy slices. The model clip (ADR-0177) is applied by the caller on `R`.
pub fn palw_bond_budget_reservation_v1(policy: &PalwBondBudgetPolicyV1, ask: PalwBudgetAskV1) -> PalwBudgetVectorV1 {
    let mut v = PalwBudgetVectorV1 {
        claims: 1,
        block_units: ask.block_units,
        reward_sompi: ask.reward_sompi,
        final_weight: ask.final_weight,
        round_rights: 0,
    };
    if let Some((r_claim, f_claim)) = palw_bond_budget_claim_ceilings_v1(policy) {
        v.reward_sompi = v.reward_sompi.min(r_claim);
        v.final_weight = v.final_weight.min(f_claim);
    }
    v
}

/// **Admission** (design §2.5): the open-claim cap, then `window + reservation ≤ caps(C)` in `Q, B, R, F` order.
pub fn palw_bond_budget_admit_v1(
    policy: &PalwBondBudgetPolicyV1,
    capital: u64,
    row: Option<&PalwBudgetBondRowV1>,
    reservation: &PalwBudgetVectorV1,
    liability_sompi: u128,
) -> Result<(), PalwBudgetRefusalV1> {
    let row = row.copied().unwrap_or_default();
    if row.open_claims >= policy.max_open_claims_per_bond {
        return Err(PalwBudgetRefusalV1::OpenClaims);
    }
    let caps = palw_bond_budget_caps_v1(policy, capital);
    // PESG §6: the open claims' liability collateral within the capital (open ≤ ⌊C/K⌋ for a common K).
    let liability = row.open_liability_sompi.checked_add(liability_sompi).ok_or(PalwBudgetRefusalV1::Overflow)?;
    if liability > capital as u128 {
        return Err(PalwBudgetRefusalV1::LiabilityExceedsCapital);
    }
    // PESG §6: the unsettled weight — every open claim's Final-weight reservation, in or out of its window — within F_max. (A claim
    // still open past `d + W` has left the window but not the unsettled weight.)
    let unsettled = row.open_final_weight.checked_add(reservation.final_weight).ok_or(PalwBudgetRefusalV1::Overflow)?;
    if unsettled > caps.final_weight {
        return Err(PalwBudgetRefusalV1::UnsettledWeight);
    }
    let after = row.window.checked_add(*reservation).ok_or(PalwBudgetRefusalV1::Overflow)?;
    if let Some(dim) = after.first_excess(&caps) {
        return Err(PalwBudgetRefusalV1::Exhausted { dim });
    }
    // The value-of-weight slot: `reward + weight · v` within R_max, once the value is known.
    if let PalwWeightValuePolicyV1::SompiPerWeight { sompi_per_weight } = policy.weight_value {
        let valued = after.final_weight.saturating_mul(sompi_per_weight as u128).saturating_add(after.reward_sompi as u128);
        if valued > caps.reward_sompi as u128 {
            return Err(PalwBudgetRefusalV1::Exhausted { dim: PalwBudgetDimV1::Reward });
        }
    }
    Ok(())
}

/// **PESG §6: the most of a claim's reward that may leave the chain's books** while its liability holds `held` collateral:
/// `⌊held · export_cap_permille / 1000⌋` (at most 0.51 of it).
pub fn palw_bond_budget_export_cap_v1(policy: &PalwBondBudgetPolicyV1, held: u128) -> u64 {
    let cap = held.saturating_mul(policy.export_cap_permille.min(PALW_BOND_BUDGET_EXPORT_CAP_MAX_PERMILLE_V1) as u128) / 1_000;
    u64::try_from(cap).unwrap_or(u64::MAX)
}

/// **A rider batch's attribution of its one physical block** (design §2.1): each of `riders` gets `⌊U/(1+n)⌋`, the lead keeps the exact
/// rest — the deterministic remainder is the lead's, and the sum is one block.
pub fn palw_rider_block_attribution_v1(riders: u64) -> (u64, u64) {
    let each = PALW_BUDGET_BLOCK_UNIT_V1 / riders.saturating_add(1);
    (PALW_BUDGET_BLOCK_UNIT_V1 - each.saturating_mul(riders), each)
}

/// **A model's available reward budget** (design §3.4): `⌊accrued · A_m / ΣA⌋ − reserved_m`, exact in 512 bits; `0` when `ΣA = 0` or
/// `A_m = 0` (the zero denominator rule). `A_m ≤ ΣA` keeps the share within `accrued`, and the floors keep `Σ_m` share within it too:
/// the remainders are never allocated (and never minted, P-7).
pub fn palw_model_available_v1(accrued: u64, a_m: &PalwAllocationWeightV1, sum_a: &PalwAllocationWeightV1, reserved_m: u64) -> u64 {
    if sum_a.is_zero() || a_m.is_zero() {
        return 0;
    }
    let (product, overflow) = a_m.overflowing_mul_u64(accrued);
    if overflow {
        // Unreachable under PALW_ALLOCATION_ALPHA_HALVES_MAX_V1 (A_m < 2^256, accrued < 2^64); refuse to allocate rather than guess.
        return 0;
    }
    let (share, _) = product.div_rem(*sum_a);
    let share = if share > PalwAllocationWeightV1::from_u64(accrued) { accrued } else { share.as_u64() };
    share.saturating_sub(reserved_m)
}

/// **Pro-rata clip** of a bond's amounts to its capital (design §3.2): unchanged when `Σ ≤ C`, else each `⌊a·C/Σ⌋`; zeros dropped. The
/// result never sums above `C`.
pub fn palw_assignment_pro_rata_v1(amounts: &[(Hash64, u64)], capital: u64) -> Vec<(Hash64, u64)> {
    let total: u128 = amounts.iter().map(|(_, a)| *a as u128).sum();
    if total <= capital as u128 {
        return amounts.iter().copied().filter(|(_, a)| *a > 0).collect();
    }
    amounts.iter().map(|(m, a)| (*m, floor_mul_div(*a as u128, capital as u128, total) as u64)).filter(|(_, a)| *a > 0).collect()
}

/// **The amounts a bond's assignment row counts for an epoch** (design §3.2, after promotion): the effective amounts, each lowered to
/// the pending amount when an unseasoned assignment is pending (a decrease applies at once, an increase waits), then pro-rata clipped to
/// the bond's capital; nothing for a bond that is not Active.
pub fn palw_assignment_amounts_v1(row: &PalwCapitalAssignmentRowV1, capital: u64, active: bool) -> Vec<(Hash64, u64)> {
    if !active || capital == 0 {
        return Vec::new();
    }
    let amounts: Vec<(Hash64, u64)> = match &row.pending {
        None => row.effective.clone(),
        Some(pending) => {
            let pending: BTreeMap<Hash64, u64> = pending.iter().copied().collect();
            row.effective.iter().map(|(m, a)| (*m, (*a).min(pending.get(m).copied().unwrap_or(0)))).collect()
        }
    };
    palw_assignment_pro_rata_v1(&amounts, capital)
}

/// The message a capital assignment's signer signs: `H(domain; network ‖ bond ‖ assignments ‖ sequence)`.
pub fn palw_capital_assignment_message_v1(
    network_domain: Hash64,
    bond: &PalwBondKeyV2,
    assignments: &[(Hash64, u64)],
    sequence: u64,
) -> Hash64 {
    keyed64(
        PALW_CAPITAL_ASSIGNMENT_MESSAGE_DOMAIN_V1,
        &[
            network_domain.as_byte_slice(),
            &borsh::to_vec(bond).expect("a bond key serializes"),
            &borsh::to_vec(&assignments.to_vec()).expect("assignments serialize"),
            &sequence.to_le_bytes(),
        ],
    )
}

fn keyed64(domain: &[u8], parts: &[&[u8]]) -> Hash64 {
    let mut s = blake2b_simd::Params::new().hash_length(64).key(domain).to_state();
    for part in parts {
        s.update(&(part.len() as u64).to_le_bytes());
        s.update(part);
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(s.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// Trusted UTXO-derived fee income of one accepted Round's native body. Both
/// amounts exclude mandatory burns; no future transaction fee is promised.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwRoundFeeIncomeV1 {
    pub block: Hash64,
    pub bond: PalwBondKeyV2,
    pub maximum_sompi: u64,
    pub actual_sompi: u64,
}

pub fn palw_round_income_id_v1(block: Hash64, bond: &PalwBondKeyV2) -> Hash64 {
    keyed64(b"misaka-palw/bond-budget/round-income/v1", &[block.as_byte_slice(), &borsh::to_vec(bond).expect("bond serializes")])
}

/// Disposable native acceptance preview, seeded/clocked by the same engine as
/// the financial fold. The book is not serialized into the virtual state.
#[derive(Clone, Debug)]
pub struct PalwRoundFeeAdmissionV1 {
    pub policy: PalwBondBudgetPolicyV1,
    pub book: PalwBondBudgetStateV1,
    pub capital: BTreeMap<PalwBondKeyV2, u64>,
    pub daa_score: u64,
}

impl PalwRoundFeeAdmissionV1 {
    /// All-or-nothing preview before the native body changes UTXOs/MuHash.
    pub fn admit(&mut self, income: PalwRoundFeeIncomeV1) -> Result<(), PalwBudgetRefusalV1> {
        let capital =
            self.capital.get(&income.bond).copied().ok_or(PalwBudgetRefusalV1::Assignment("Round fee recipient is not bonded"))?;
        if income.maximum_sompi == 0 && income.actual_sompi == 0 {
            return Ok(());
        }
        self.book.reserve_income_v1(
            &self.policy,
            capital,
            palw_round_income_id_v1(income.block, &income.bond),
            income.bond,
            self.daa_score,
            income.maximum_sompi,
            income.actual_sompi,
            PalwBudgetOriginV1::RoundFee,
            &mut Vec::new(),
        )
    }
}

// ---- the state -----------------------------------------------------------------------------------------------------------------

/// The engine header (delta 191).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwBondBudgetHeaderV1 {
    pub version: u16,
    /// The block that created the engine (its first block at or past the fence).
    pub created_daa: u64,
    /// The budget policy the engine was created under.
    pub policy_digest: Hash64,
    /// ADR-0177: the current allocation epoch (`None` below the allocation fence).
    pub allocation: Option<PalwAllocationEpochV1>,
}

/// The current allocation epoch's frozen snapshot totals and its realized PALW budget so far.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwAllocationEpochV1 {
    pub index: u64,
    pub start_daa: u64,
    /// `Σ_m A_m` of the snapshot (exact).
    pub sum_a: PalwAllocationWeightV1,
    /// The worker carve of every chain block of this epoch so far (`R_PALW(t)` as realized).
    pub accrued_sompi: u64,
}

/// A bond's window (table 1): the reservations still inside `W`, its open budgeted claims, and the latest `reuse_not_before` among them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwBudgetBondRowV1 {
    pub window: PalwBudgetVectorV1,
    pub open_claims: u32,
    pub latest_release_daa: u64,
    /// PESG §6: `Σ K` over the bond's open claims.
    pub open_liability_sompi: u128,
    /// PESG §6: `Σ` Final-weight reservation over the bond's open claims (its unsettled weight's bound).
    pub open_final_weight: u128,
    /// PESG §6: the liability hold — the bond's capital stays held until this DAA (latest terminal + `H_L`); 0 when none.
    pub liability_hold_until: u64,
}

/// A claim's reservation (table 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwBudgetClaimRowV1 {
    pub bond: PalwBondKeyV2,
    /// ADR-0177: `(model, allocation epoch)` fixed at acceptance; `None` below the allocation fence.
    pub model: Option<(Hash64, u64)>,
    pub accepted_daa: u64,
    pub reuse_not_before: u64,
    pub reserved: PalwBudgetVectorV1,
    pub consumed: PalwBudgetVectorV1,
    /// `false` once the claim is terminal (Final paid, void, conviction, retired): no further consumption.
    pub open: bool,
    /// `false` once released from the window at `reuse_not_before`.
    pub in_window: bool,
    /// `false` once the V2 claim has left the state (retired): the row is dropped once it is also out of the window. While it is live,
    /// its consumption answers what the claim was granted (rule E's reader, hook H-4).
    pub live: bool,
    pub origin: PalwBudgetOriginV1,
    /// PESG §6: the liability collateral `K` the claim reserved (its V2 `reserved`).
    pub liability_sompi: u128,
}

/// A bond's capital assignment (table 4; ADR-0177 D3).
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwCapitalAssignmentRowV1 {
    /// The last accepted sequence (replay protection).
    pub sequence: u64,
    /// The amounts counted in snapshots (seasoned).
    pub effective: Vec<(Hash64, u64)>,
    /// The latest accepted assignment, waiting to season.
    pub pending: Option<Vec<(Hash64, u64)>>,
    /// The epoch the pending assignment was accepted in.
    pub pending_since_epoch: u64,
}

/// A model's snapshot and its reservations this epoch (table 5).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwModelBudgetRowV1 {
    pub epoch: u64,
    /// `S_m`.
    pub capital: u128,
    /// `A_m = S_m^α` (exact).
    pub weight: PalwAllocationWeightV1,
    pub reserved_sompi: u64,
}

/// A frozen native reporter income binding. `None` explicitly means legacy
/// cash whose actual recipient cannot be authenticated from the old carriage;
/// it is quarantined globally, never assigned by matching payout payloads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwReporterIncomeBindingV1 {
    pub recipient: Option<PalwBondKeyV2>,
    pub payload: Hash64,
    pub maximum: u64,
    pub accepted_daa: u64,
    pub awarded_daa: Option<u64>,
    pub queue: Option<Hash64>,
    pub paid_daa: Option<u64>,
    /// Historical awarded income with no physical clock in the old carriage:
    /// `(observed cumulative amount, conservative first-fence+W expiry)`.
    /// This is an uncertainty boundary, never a fabricated payment timestamp.
    pub historical_boundary: Option<(u128, u64)>,
}

impl PalwReporterIncomeBindingV1 {
    fn valid(&self) -> bool {
        if self.awarded_daa.is_some_and(|daa| daa < self.accepted_daa) || self.paid_daa.is_some_and(|daa| daa < self.accepted_daa) {
            return false;
        }
        match (self.recipient, self.historical_boundary) {
            (Some(_), None) => self.maximum > 0 && self.queue.is_none() && self.paid_daa.is_none(),
            (None, None) => self.maximum > 0 && self.queue.is_some() && self.awarded_daa.is_none(),
            (None, Some((amount, until))) => {
                amount > 0
                    && until >= self.accepted_daa
                    && self.maximum == 0
                    && self.queue.is_none()
                    && self.paid_daa.is_none()
                    && self.awarded_daa.is_none()
            }
            _ => false,
        }
    }
}

/// **The engine state** (`PalwChainStateV2::bond_budget`): the header and five tables. Borsh for the carriage (tail `0xEF`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwBondBudgetStateV1 {
    pub header: PalwBondBudgetHeaderV1,
    pub bonds: BTreeMap<PalwBondKeyV2, PalwBudgetBondRowV1>,
    pub claims: BTreeMap<Hash64, PalwBudgetClaimRowV1>,
    pub releases: BTreeSet<(u64, Hash64)>,
    pub assignments: BTreeMap<PalwBondKeyV2, PalwCapitalAssignmentRowV1>,
    pub models: BTreeMap<Hash64, PalwModelBudgetRowV1>,
    /// PESG §6: the liability holds' expiries.
    pub liability_releases: BTreeSet<(u64, PalwBondKeyV2)>,
    pub reporter_income: BTreeMap<Hash64, PalwReporterIncomeBindingV1>,
    pub material_income: BTreeMap<Hash64, PalwMaterialIncomeV1>,
    pub consumer_contracts: BTreeMap<Hash64, PalwPaidConsumerContractV1>,
    consumer_service_due: BTreeSet<(u64, Hash64)>,
    consumer_exhausted: BTreeSet<Hash64>,
    material_roots: BTreeMap<Option<Hash64>, BTreeSet<Hash64>>,
    material_sources: BTreeMap<Hash64, BTreeSet<Hash64>>,
    material_due: BTreeSet<(u64, Hash64)>,
    /// Derived withdrawal hold: no extra serialized/rooted field or history scan.
    deferred_by_bond: BTreeMap<PalwBondKeyV2, usize>,
    deferred_awards: BTreeSet<Hash64>,
    unattributed_income: usize,
    pending_reporters: BTreeMap<PalwBondKeyV2, usize>,
}

impl BorshSerialize for PalwBondBudgetStateV1 {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        self.header.serialize(writer)?;
        self.bonds.serialize(writer)?;
        self.claims.serialize(writer)?;
        self.releases.serialize(writer)?;
        self.assignments.serialize(writer)?;
        self.models.serialize(writer)?;
        // PESG §6 (integration): the liability holds' expiries, in every monetary version, after the five original tables.
        self.liability_releases.serialize(writer)?;
        match self.header.version {
            PALW_BOND_BUDGET_STATE_VERSION_V1
                if self.reporter_income.is_empty() && self.material_income.is_empty() && self.consumer_contracts.is_empty() =>
            {
                Ok(())
            }
            PALW_BOND_BUDGET_STATE_VERSION_V2 if self.material_income.is_empty() && self.consumer_contracts.is_empty() => {
                self.reporter_income.serialize(writer)
            }
            PALW_BOND_BUDGET_STATE_VERSION_V3 if self.consumer_contracts.is_empty() => {
                self.reporter_income.serialize(writer)?;
                self.material_income.serialize(writer)
            }
            PALW_BOND_BUDGET_STATE_VERSION_V4 => {
                self.reporter_income.serialize(writer)?;
                self.material_income.serialize(writer)?;
                self.consumer_contracts.serialize(writer)
            }
            _ => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid budget monetary state version")),
        }
    }
}

impl BorshDeserialize for PalwBondBudgetStateV1 {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let header: PalwBondBudgetHeaderV1 = BorshDeserialize::deserialize_reader(reader)?;
        if !matches!(
            header.version,
            PALW_BOND_BUDGET_STATE_VERSION_V1
                | PALW_BOND_BUDGET_STATE_VERSION_V2
                | PALW_BOND_BUDGET_STATE_VERSION_V3
                | PALW_BOND_BUDGET_STATE_VERSION_V4
        ) {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid budget monetary state version"));
        }
        let version = header.version;
        let mut state = Self {
            header,
            bonds: BorshDeserialize::deserialize_reader(reader)?,
            claims: BorshDeserialize::deserialize_reader(reader)?,
            releases: BorshDeserialize::deserialize_reader(reader)?,
            assignments: BorshDeserialize::deserialize_reader(reader)?,
            models: BorshDeserialize::deserialize_reader(reader)?,
            liability_releases: BorshDeserialize::deserialize_reader(reader)?,
            reporter_income: if version >= PALW_BOND_BUDGET_STATE_VERSION_V2 {
                BorshDeserialize::deserialize_reader(reader)?
            } else {
                BTreeMap::new()
            },
            material_income: if version >= PALW_BOND_BUDGET_STATE_VERSION_V3 {
                BorshDeserialize::deserialize_reader(reader)?
            } else {
                BTreeMap::new()
            },
            consumer_contracts: if version >= PALW_BOND_BUDGET_STATE_VERSION_V4 {
                BorshDeserialize::deserialize_reader(reader)?
            } else {
                BTreeMap::new()
            },
            consumer_service_due: BTreeSet::new(),
            consumer_exhausted: BTreeSet::new(),
            material_roots: BTreeMap::new(),
            material_sources: BTreeMap::new(),
            material_due: BTreeSet::new(),
            deferred_by_bond: BTreeMap::new(),
            deferred_awards: BTreeSet::new(),
            unattributed_income: 0,
            pending_reporters: BTreeMap::new(),
        };
        let consumers: Vec<_> = state.consumer_contracts.iter().map(|(id, row)| (*id, *row)).collect();
        for (id, row) in consumers {
            state.consumer_index_replace_v1(id, None, Some(&row));
        }
        let materials: Vec<_> = state.material_income.iter().map(|(id, row)| (*id, *row)).collect();
        for (id, row) in materials {
            state.material_index_replace_v1(id, None, Some(&row));
        }
        for (id, row) in &state.claims {
            if Self::unpaid_reporter_debt(row) {
                *state.deferred_by_bond.entry(row.bond).or_default() += 1;
                state.deferred_awards.insert(*id);
            }
        }
        state.unattributed_income = state.reporter_income.values().filter(|row| row.recipient.is_none()).count();
        for row in state.reporter_income.values().filter(|row| row.awarded_daa.is_none()) {
            if let Some(bond) = row.recipient {
                *state.pending_reporters.entry(bond).or_default() += 1;
            }
        }
        Ok(state)
    }
}

/// One journaled write of the engine (the fold turns `Header` into delta 191 and `Row` into delta 190).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwBudgetWriteV1 {
    Header { old: Option<PalwBondBudgetHeaderV1>, new: Option<PalwBondBudgetHeaderV1> },
    Row { table: u8, key: Vec<u8>, old: Option<Vec<u8>>, new: Option<Vec<u8>> },
}

fn enc<T: BorshSerialize>(value: &T) -> Vec<u8> {
    borsh::to_vec(value).expect("budget rows serialize")
}

fn put<K: Ord + Clone + BorshSerialize, V: PartialEq + Clone + BorshSerialize>(
    map: &mut BTreeMap<K, V>,
    table: u8,
    key: &K,
    new: Option<V>,
    j: &mut Vec<PalwBudgetWriteV1>,
) {
    let old = match &new {
        Some(v) => map.insert(key.clone(), v.clone()),
        None => map.remove(key),
    };
    if old != new {
        j.push(PalwBudgetWriteV1::Row { table, key: enc(key), old: old.as_ref().map(enc), new: new.as_ref().map(enc) });
    }
}

fn swap_row<K: Ord + BorshDeserialize + BorshSerialize, V: BorshDeserialize + BorshSerialize>(
    map: &mut BTreeMap<K, V>,
    key: &[u8],
    expected: &Option<Vec<u8>>,
    install: &Option<Vec<u8>>,
) -> Result<(), &'static str> {
    let k: K = borsh::from_slice(key).map_err(|_| "a budget row's key does not decode")?;
    if map.get(&k).map(enc) != *expected {
        return Err("a budget row does not match the delta's expectation");
    }
    match install {
        Some(bytes) => {
            map.insert(k, borsh::from_slice(bytes).map_err(|_| "a budget row does not decode")?);
        }
        None => {
            map.remove(&k);
        }
    }
    Ok(())
}

impl PalwBondBudgetStateV1 {
    /// A new engine (the fence's first block).
    pub fn new(created_daa: u64, policy: &PalwBondBudgetPolicyV1) -> Self {
        Self {
            header: PalwBondBudgetHeaderV1 {
                version: PALW_BOND_BUDGET_STATE_VERSION_V1,
                created_daa,
                policy_digest: policy.digest(),
                allocation: None,
            },
            bonds: BTreeMap::new(),
            claims: BTreeMap::new(),
            releases: BTreeSet::new(),
            assignments: BTreeMap::new(),
            models: BTreeMap::new(),
            liability_releases: BTreeSet::new(),
            reporter_income: BTreeMap::new(),
            consumer_contracts: BTreeMap::new(),
            consumer_service_due: BTreeSet::new(),
            consumer_exhausted: BTreeSet::new(),
            material_income: BTreeMap::new(),
            material_roots: BTreeMap::new(),
            material_sources: BTreeMap::new(),
            material_due: BTreeSet::new(),
            deferred_by_bond: BTreeMap::new(),
            deferred_awards: BTreeSet::new(),
            unattributed_income: 0,
            pending_reporters: BTreeMap::new(),
        }
    }

    /// The engine as a header alone (delta 191 creating it on application).
    pub fn from_header(header: PalwBondBudgetHeaderV1) -> Self {
        Self {
            header,
            bonds: BTreeMap::new(),
            claims: BTreeMap::new(),
            releases: BTreeSet::new(),
            assignments: BTreeMap::new(),
            models: BTreeMap::new(),
            liability_releases: BTreeSet::new(),
            reporter_income: BTreeMap::new(),
            consumer_contracts: BTreeMap::new(),
            consumer_service_due: BTreeSet::new(),
            consumer_exhausted: BTreeSet::new(),
            material_income: BTreeMap::new(),
            material_roots: BTreeMap::new(),
            material_sources: BTreeMap::new(),
            material_due: BTreeSet::new(),
            deferred_by_bond: BTreeMap::new(),
            deferred_awards: BTreeSet::new(),
            unattributed_income: 0,
            pending_reporters: BTreeMap::new(),
        }
    }

    // ---- readers ----

    pub fn bond_row(&self, bond: &PalwBondKeyV2) -> Option<&PalwBudgetBondRowV1> {
        self.bonds.get(bond)
    }

    pub fn claim_row(&self, claim: &Hash64) -> Option<&PalwBudgetClaimRowV1> {
        self.claims.get(claim)
    }

    /// Deterministic pending awards, without scanning completed monetary history.
    pub fn unpaid_reporter_awards_v1(&self) -> impl Iterator<Item = &Hash64> {
        self.deferred_awards.iter()
    }

    pub fn model_row(&self, model: &Hash64) -> Option<&PalwModelBudgetRowV1> {
        self.models.get(model)
    }

    pub fn assignment_row(&self, bond: &PalwBondKeyV2) -> Option<&PalwCapitalAssignmentRowV1> {
        self.assignments.get(bond)
    }

    /// **Does the bond still hold a reservation inside its window at `now`?** (the withdrawal hold, design §2.4) — exact: the latest
    /// `reuse_not_before` among its in-window reservations is still ahead.
    /// A pending monetary right uses `u64::MAX` until the ordinary release
    /// clock removes its last reservation, because its payment DAA is unknown.
    pub fn window_holds(&self, bond: &PalwBondKeyV2, now: u64) -> bool {
        self.deferred_by_bond.contains_key(bond)
            || self.pending_reporters.contains_key(bond)
            || self.bonds.get(bond).is_some_and(|row| !row.window.is_zero() && row.latest_release_daa > now)
    }

    /// **PESG §6: does the bond's liability hold its capital at `now`?** — an open claim, or a terminal one within `H_L`. A clock of its
    /// own, apart from the issuance window.
    pub fn liability_holds(&self, bond: &PalwBondKeyV2, now: u64) -> bool {
        self.bonds.get(bond).is_some_and(|row| row.open_claims > 0 || row.liability_hold_until > now)
    }

    /// **The withdrawal hold**: the issuance window (`d + W`) or the liability hold (`H_L`), whichever is longer.
    pub fn withdrawal_holds(&self, bond: &PalwBondKeyV2, now: u64) -> bool {
        self.window_holds(bond, now) || self.liability_holds(bond, now)
    }

    /// PESG §6: the bond's unsettled weight bound — the Final-weight reservations of its open claims.
    pub fn unsettled_final_weight(&self, bond: &PalwBondKeyV2) -> u128 {
        self.bonds.get(bond).map(|row| row.open_final_weight).unwrap_or(0)
    }

    /// The model `m`'s available reward budget in the current epoch (0 with no allocation, no row, or an old row).
    pub fn model_available(&self, model: &Hash64) -> u64 {
        let Some(epoch) = self.header.allocation else { return 0 };
        let Some(row) = self.models.get(model).filter(|row| row.epoch == epoch.index) else { return 0 };
        palw_model_available_v1(epoch.accrued_sompi, &row.weight, &epoch.sum_a, row.reserved_sompi)
    }

    /// The table roots and the header digest the V2 root block writes, in table order.
    pub fn root_parts(&self) -> (Hash64, [Hash64; 6]) {
        use crate::palw_state_v2::palw_collection_root_of_entries_v1 as root;
        let header = if self.header.version >= PALW_BOND_BUDGET_STATE_VERSION_V2 {
            let income = root(
                b"bond_budget_reporter_income/v1",
                self.reporter_income.len(),
                self.reporter_income.iter().map(|(k, v)| (enc(k), enc(v))),
            );
            if self.header.version >= PALW_BOND_BUDGET_STATE_VERSION_V3 {
                let material = root(
                    b"bond_budget_material_income/v1",
                    self.material_income.len(),
                    self.material_income.iter().map(|(k, v)| (enc(k), enc(v))),
                );
                if self.header.version >= PALW_BOND_BUDGET_STATE_VERSION_V4 {
                    let consumers = root(
                        b"bond_budget_consumer_contract/v1",
                        self.consumer_contracts.len(),
                        self.consumer_contracts.iter().map(|(k, v)| (enc(k), enc(v))),
                    );
                    keyed64(
                        PALW_BOND_BUDGET_HEADER_DOMAIN_V1,
                        &[
                            &enc(&self.header),
                            b"reporter_income/v1",
                            income.as_byte_slice(),
                            b"material_income/v1",
                            material.as_byte_slice(),
                            b"consumer_contract/v1",
                            consumers.as_byte_slice(),
                        ],
                    )
                } else {
                    keyed64(
                        PALW_BOND_BUDGET_HEADER_DOMAIN_V1,
                        &[
                            &enc(&self.header),
                            b"reporter_income/v1",
                            income.as_byte_slice(),
                            b"material_income/v1",
                            material.as_byte_slice(),
                        ],
                    )
                }
            } else {
                keyed64(PALW_BOND_BUDGET_HEADER_DOMAIN_V1, &[&enc(&self.header), b"reporter_income/v1", income.as_byte_slice()])
            }
        } else {
            keyed64(PALW_BOND_BUDGET_HEADER_DOMAIN_V1, &[&enc(&self.header)])
        };
        (
            header,
            [
                root(b"bond_budget_bonds", self.bonds.len(), self.bonds.iter().map(|(k, v)| (enc(k), enc(v)))),
                root(b"bond_budget_claims", self.claims.len(), self.claims.iter().map(|(k, v)| (enc(k), enc(v)))),
                root(b"bond_budget_releases", self.releases.len(), self.releases.iter().map(|k| (enc(k), Vec::new()))),
                root(b"bond_budget_assignments", self.assignments.len(), self.assignments.iter().map(|(k, v)| (enc(k), enc(v)))),
                root(b"bond_budget_models", self.models.len(), self.models.iter().map(|(k, v)| (enc(k), enc(v)))),
                root(
                    b"bond_budget_liability_releases",
                    self.liability_releases.len(),
                    self.liability_releases.iter().map(|k| (enc(k), Vec::new())),
                ),
            ],
        )
    }

    // ---- delta application (verify-then-install; `revert` swaps old and new) ----

    pub fn apply_row(
        &mut self,
        table: u8,
        key: &[u8],
        old: &Option<Vec<u8>>,
        new: &Option<Vec<u8>>,
        revert: bool,
    ) -> Result<(), &'static str> {
        let (expected, install) = if revert { (new, old) } else { (old, new) };
        match table {
            PALW_BUDGET_TABLE_BONDS_V1 => swap_row(&mut self.bonds, key, expected, install),
            PALW_BUDGET_TABLE_CLAIMS_V1 => {
                let id: Hash64 = borsh::from_slice(key).map_err(|_| "a budget row's key does not decode")?;
                let old = self.claims.get(&id).copied();
                swap_row(&mut self.claims, key, expected, install)?;
                let new = self.claims.get(&id).copied();
                self.update_deferred_index(id, old.as_ref(), new.as_ref());
                Ok(())
            }
            PALW_BUDGET_TABLE_RELEASES_V1 => {
                let k: (u64, Hash64) = borsh::from_slice(key).map_err(|_| "a release key does not decode")?;
                let present = self.releases.contains(&k);
                if present != expected.is_some() {
                    return Err("a release row does not match the delta's expectation");
                }
                if install.is_some() {
                    self.releases.insert(k);
                } else {
                    self.releases.remove(&k);
                }
                Ok(())
            }
            PALW_BUDGET_TABLE_ASSIGNMENTS_V1 => swap_row(&mut self.assignments, key, expected, install),
            PALW_BUDGET_TABLE_MODELS_V1 => swap_row(&mut self.models, key, expected, install),
            PALW_BUDGET_TABLE_LIABILITY_RELEASES_V1 => {
                let k: (u64, PalwBondKeyV2) = borsh::from_slice(key).map_err(|_| "a liability release key does not decode")?;
                if self.liability_releases.contains(&k) != expected.is_some() {
                    return Err("a liability release row does not match the delta's expectation");
                }
                if install.is_some() {
                    self.liability_releases.insert(k);
                } else {
                    self.liability_releases.remove(&k);
                }
                Ok(())
            }
            PALW_BUDGET_TABLE_REPORTER_INCOME_V1 => {
                if self.header.version < PALW_BOND_BUDGET_STATE_VERSION_V2 {
                    return Err("reporter income needs monetary state v2");
                }
                let id: Hash64 = borsh::from_slice(key).map_err(|_| "reporter income key does not decode")?;
                let old = self.reporter_income.get(&id).copied();
                swap_row(&mut self.reporter_income, key, expected, install)?;
                let new = self.reporter_income.get(&id).copied();
                self.update_reporter_binding_index_v1(old.as_ref(), new.as_ref());
                Ok(())
            }
            PALW_BUDGET_TABLE_MATERIAL_INCOME_V1 => {
                if self.header.version < PALW_BOND_BUDGET_STATE_VERSION_V3 {
                    return Err("material income needs monetary state v3");
                }
                let id: Hash64 = borsh::from_slice(key).map_err(|_| "material income key does not decode")?;
                let old = self.material_income.get(&id).copied();
                swap_row(&mut self.material_income, key, expected, install)?;
                let new = self.material_income.get(&id).copied();
                self.material_index_replace_v1(id, old.as_ref(), new.as_ref());
                Ok(())
            }
            PALW_BUDGET_TABLE_CONSUMER_CONTRACT_V1 => {
                if self.header.version < PALW_BOND_BUDGET_STATE_VERSION_V4 {
                    return Err("consumer contract needs monetary state v4");
                }
                let id: Hash64 = borsh::from_slice(key).map_err(|_| "consumer contract key does not decode")?;
                let old = self.consumer_contracts.get(&id).copied();
                swap_row(&mut self.consumer_contracts, key, expected, install)?;
                let new = self.consumer_contracts.get(&id).copied();
                self.consumer_index_replace_v1(id, old.as_ref(), new.as_ref());
                Ok(())
            }
            _ => Err("a budget row names an unknown table"),
        }
    }

    /// Whether the engine holds no row (a header-only engine may be dropped by a revert of its creation).
    pub fn has_no_rows(&self) -> bool {
        self.bonds.is_empty()
            && self.claims.is_empty()
            && self.releases.is_empty()
            && self.assignments.is_empty()
            && self.models.is_empty()
            && self.liability_releases.is_empty()
            && self.reporter_income.is_empty()
            && self.material_income.is_empty()
            && self.consumer_contracts.is_empty()
    }

    // ---- journaled writers ----

    fn put_liability_release(&mut self, key: (u64, PalwBondKeyV2), present: bool, j: &mut Vec<PalwBudgetWriteV1>) {
        let was = if present { !self.liability_releases.insert(key) } else { self.liability_releases.remove(&key) };
        if was != present {
            j.push(PalwBudgetWriteV1::Row {
                table: PALW_BUDGET_TABLE_LIABILITY_RELEASES_V1,
                key: enc(&key),
                old: was.then(Vec::new),
                new: present.then(Vec::new),
            });
        }
    }

    fn put_bond(&mut self, bond: &PalwBondKeyV2, row: PalwBudgetBondRowV1, j: &mut Vec<PalwBudgetWriteV1>) {
        let keep = !row.window.is_zero() || row.open_claims > 0 || row.liability_hold_until > 0;
        put(&mut self.bonds, PALW_BUDGET_TABLE_BONDS_V1, bond, keep.then_some(row), j);
    }

    fn unpaid_reporter_debt(row: &PalwBudgetClaimRowV1) -> bool {
        row.origin == PalwBudgetOriginV1::DeferredReporterIncome && row.live && row.consumed.reward_sompi < row.reserved.reward_sompi
    }

    fn update_deferred_index(&mut self, id: Hash64, old: Option<&PalwBudgetClaimRowV1>, new: Option<&PalwBudgetClaimRowV1>) {
        if let Some(old) = old.filter(|row| Self::unpaid_reporter_debt(row)) {
            self.deferred_awards.remove(&id);
            if let Some(count) = self.deferred_by_bond.get_mut(&old.bond) {
                *count -= 1;
                if *count == 0 {
                    self.deferred_by_bond.remove(&old.bond);
                }
            }
        }
        if let Some(new) = new.filter(|row| Self::unpaid_reporter_debt(row)) {
            self.deferred_awards.insert(id);
            *self.deferred_by_bond.entry(new.bond).or_default() += 1;
        }
    }

    fn put_claim(&mut self, claim: &Hash64, row: Option<PalwBudgetClaimRowV1>, j: &mut Vec<PalwBudgetWriteV1>) {
        let old = self.claims.get(claim).copied();
        self.update_deferred_index(*claim, old.as_ref(), row.as_ref());
        put(&mut self.claims, PALW_BUDGET_TABLE_CLAIMS_V1, claim, row, j);
    }

    fn put_release(&mut self, key: (u64, Hash64), present: bool, j: &mut Vec<PalwBudgetWriteV1>) {
        let was = if present { !self.releases.insert(key) } else { self.releases.remove(&key) };
        if was != present {
            j.push(PalwBudgetWriteV1::Row {
                table: PALW_BUDGET_TABLE_RELEASES_V1,
                key: enc(&key),
                old: was.then(Vec::new),
                new: present.then(Vec::new),
            });
        }
    }

    fn update_reporter_binding_index_v1(
        &mut self,
        old: Option<&PalwReporterIncomeBindingV1>,
        new: Option<&PalwReporterIncomeBindingV1>,
    ) {
        if let Some(old) = old {
            if old.recipient.is_none() {
                self.unattributed_income -= 1;
            }
            if old.awarded_daa.is_none()
                && let Some(bond) = old.recipient
                && let Some(count) = self.pending_reporters.get_mut(&bond)
            {
                *count -= 1;
                if *count == 0 {
                    self.pending_reporters.remove(&bond);
                }
            }
        }
        if let Some(new) = new {
            if new.recipient.is_none() {
                self.unattributed_income += 1;
            }
            if new.awarded_daa.is_none()
                && let Some(bond) = new.recipient
            {
                *self.pending_reporters.entry(bond).or_default() += 1;
            }
        }
    }

    pub fn activate_reporter_income_v2(&mut self, j: &mut Vec<PalwBudgetWriteV1>) {
        if self.header.version < PALW_BOND_BUDGET_STATE_VERSION_V2 {
            let mut header = self.header.clone();
            header.version = PALW_BOND_BUDGET_STATE_VERSION_V2;
            self.put_header(header, j);
        }
    }

    fn put_reporter_income_v1(&mut self, id: &Hash64, row: Option<PalwReporterIncomeBindingV1>, j: &mut Vec<PalwBudgetWriteV1>) {
        self.activate_reporter_income_v2(j);
        let old = self.reporter_income.get(id).copied();
        self.update_reporter_binding_index_v1(old.as_ref(), row.as_ref());
        put(&mut self.reporter_income, PALW_BUDGET_TABLE_REPORTER_INCOME_V1, id, row, j);
    }

    /// An exact old native liability has no authenticated recipient. Preserve
    /// its cash and refuse only new monetary admissions until physical pay+W.
    pub fn has_unattributed_income_v1(&self) -> bool {
        self.unattributed_income > 0
    }

    /// Freeze a valid named/revealed winner's full funded maximum before award.
    /// Improving an unawarded commit-reveal winner updates only the binding;
    /// after award, its actual recipient, amount and payload are immutable.
    pub fn bind_reporter_income_v1(
        &mut self,
        id: Hash64,
        row: PalwReporterIncomeBindingV1,
        j: &mut Vec<PalwBudgetWriteV1>,
    ) -> Result<(), PalwBudgetRefusalV1> {
        if !row.valid() {
            return Err(PalwBudgetRefusalV1::Assignment("invalid reporter monetary binding"));
        }
        if let Some(old) = self.reporter_income.get(&id) {
            if *old == row {
                return Ok(());
            }
            if old.recipient.is_none()
                || old.awarded_daa.is_some()
                || old.queue.is_some()
                || old.paid_daa.is_some()
                || old.maximum != row.maximum
                || old.accepted_daa != row.accepted_daa
            {
                return Err(PalwBudgetRefusalV1::Duplicate);
            }
        }
        self.put_reporter_income_v1(&id, Some(row), j);
        Ok(())
    }

    pub fn mark_reporter_awarded_v1(
        &mut self,
        id: Hash64,
        now: u64,
        j: &mut Vec<PalwBudgetWriteV1>,
    ) -> Result<(), PalwBudgetRefusalV1> {
        let mut row = self.reporter_income.get(&id).copied().ok_or(PalwBudgetRefusalV1::NotOpen)?;
        if row.awarded_daa.is_some() {
            return Err(PalwBudgetRefusalV1::Duplicate);
        }
        if row.recipient.is_none() || now < row.accepted_daa {
            return Err(PalwBudgetRefusalV1::Assignment("invalid reporter award clock"));
        }
        row.awarded_daa = Some(now);
        self.put_reporter_income_v1(&id, Some(row), j);
        Ok(())
    }

    /// Exactly identified old queue cash has now actually drained. Never use
    /// logical award/vesting DAA as its monetary release clock.
    pub fn confirm_unattributed_payment_v1(
        &mut self,
        id: Hash64,
        now: u64,
        j: &mut Vec<PalwBudgetWriteV1>,
    ) -> Result<(), PalwBudgetRefusalV1> {
        let mut row = self
            .reporter_income
            .get(&id)
            .copied()
            .filter(|row| row.recipient.is_none() && row.queue.is_some() && row.paid_daa.is_none())
            .ok_or(PalwBudgetRefusalV1::NotOpen)?;
        if now < row.accepted_daa {
            return Err(PalwBudgetRefusalV1::Assignment("legacy payment precedes monetary migration"));
        }
        row.paid_daa = Some(now);
        self.put_reporter_income_v1(&id, Some(row), j);
        Ok(())
    }

    pub fn release_reporter_income_v1(&mut self, policy: &PalwBondBudgetPolicyV1, now: u64, j: &mut Vec<PalwBudgetWriteV1>) {
        let due: Vec<_> = self
            .reporter_income
            .iter()
            .filter(|(_, row)| {
                row.recipient.is_none()
                    && (row.historical_boundary.is_some_and(|(_, until)| until <= now)
                        || row.paid_daa.is_some_and(|paid| paid.checked_add(policy.window_daa).is_some_and(|until| until <= now)))
            })
            .map(|(id, _)| *id)
            .collect();
        for id in due {
            self.put_reporter_income_v1(&id, None, j);
        }
    }

    fn put_header(&mut self, header: PalwBondBudgetHeaderV1, j: &mut Vec<PalwBudgetWriteV1>) {
        if self.header != header {
            let old = std::mem::replace(&mut self.header, header.clone());
            j.push(PalwBudgetWriteV1::Header { old: Some(old), new: Some(header) });
        }
    }

    /// **The clock (design §2.4)**: release every reservation with `reuse_not_before ≤ now` from its bond's window — the ONLY place a
    /// window shrinks. A released claim that is also closed leaves the table.
    pub fn release_due(&mut self, now: u64, j: &mut Vec<PalwBudgetWriteV1>) {
        let due: Vec<(u64, Hash64)> = self.releases.range(..(now.saturating_add(1), Hash64::default())).copied().collect();
        for (at, claim_id) in due {
            if at > now {
                break;
            }
            // A source promise is an unpaid liability, even if a sparse chain
            // jumps beyond H + W. Payment extends its window from the actual
            // settlement clock; the clock must never free its R beforehand.
            if self.claims.get(&claim_id).is_some_and(|claim| {
                matches!(
                    claim.origin,
                    PalwBudgetOriginV1::SourceFee
                        | PalwBudgetOriginV1::NativeIncome
                        | PalwBudgetOriginV1::NativeIncomeQueued
                        | PalwBudgetOriginV1::NativeIncomePartial
                ) && claim.live
            }) {
                continue;
            }
            self.put_release((at, claim_id), false, j);
            let Some(mut claim) = self.claims.get(&claim_id).copied() else { continue };
            let mut bond = self.bonds.get(&claim.bond).copied().unwrap_or_default();
            bond.window = bond.window.checked_sub(claim.reserved).unwrap_or(PalwBudgetVectorV1::ZERO);
            // A liability-view row ends its liability here (the only place it ends): its open count and collateral leave the view.
            if claim.origin == PalwBudgetOriginV1::HoldLiability && claim.open {
                bond.open_claims = bond.open_claims.saturating_sub(1);
                bond.open_liability_sompi = bond.open_liability_sompi.saturating_sub(claim.liability_sompi);
                claim.open = false;
            }
            self.put_bond(&claim.bond.clone(), bond, j);
            claim.in_window = false;
            self.put_claim(&claim_id, claim.live.then_some(claim), j);
        }
        // PESG §6: the liability holds that end now (a later hold of the same bond replaced its entry, so each entry is current).
        let ended: Vec<(u64, PalwBondKeyV2)> = self.liability_releases.iter().take_while(|(at, _)| *at <= now).copied().collect();
        for (at, bond) in ended {
            self.put_liability_release((at, bond), false, j);
            if let Some(mut row) = self.bonds.get(&bond).copied().filter(|row| row.liability_hold_until == at) {
                row.liability_hold_until = 0;
                self.put_bond(&bond, row, j);
            }
        }
    }

    /// **Take a reservation** (design §2.5). `admit` runs the caps and the open-claim cap (a Legacy seed does not: old claims are counted,
    /// never refused). The reservation must already carry every clip (per-claim ceilings, the model's budget).
    #[allow(clippy::too_many_arguments)]
    pub fn reserve(
        &mut self,
        policy: &PalwBondBudgetPolicyV1,
        capital: u64,
        claim_id: Hash64,
        bond: PalwBondKeyV2,
        model: Option<(Hash64, u64)>,
        accepted_daa: u64,
        reservation: PalwBudgetVectorV1,
        origin: PalwBudgetOriginV1,
        admit: bool,
        j: &mut Vec<PalwBudgetWriteV1>,
    ) -> Result<(), PalwBudgetRefusalV1> {
        self.reserve_liable(policy, capital, claim_id, bond, model, accepted_daa, reservation, 0, origin, admit, j)
    }

    /// [`Self::reserve`] for a claim that reserves `liability_sompi` of its bond's collateral (PESG §6: counted in the bond's open
    /// liability, which admission keeps within its capital).
    #[allow(clippy::too_many_arguments)]
    pub fn reserve_liable(
        &mut self,
        policy: &PalwBondBudgetPolicyV1,
        capital: u64,
        claim_id: Hash64,
        bond: PalwBondKeyV2,
        model: Option<(Hash64, u64)>,
        accepted_daa: u64,
        reservation: PalwBudgetVectorV1,
        liability_sompi: u128,
        origin: PalwBudgetOriginV1,
        admit: bool,
        j: &mut Vec<PalwBudgetWriteV1>,
    ) -> Result<(), PalwBudgetRefusalV1> {
        self.reserve_liable_moded_v1(
            policy,
            capital,
            claim_id,
            bond,
            model,
            accepted_daa,
            reservation,
            liability_sompi,
            origin,
            admit,
            PalwHoldModeV1::for_origin(origin),
            j,
        )
    }

    /// [`Self::reserve_liable`] with the claim's verification mode (ユーザー方針 2026-10-15): under a V3 policy the admission also runs
    /// [`Self::hold_admit_v1`] (day view for Panel=1, burst view, concurrent liability), and the reservation writes its shadow rows.
    /// The mode is fixed at acceptance; the long window (`reuse_not_before = d + W`, `W` = Panel=0's span) is every mode's.
    #[allow(clippy::too_many_arguments)]
    pub fn reserve_liable_moded_v1(
        &mut self,
        policy: &PalwBondBudgetPolicyV1,
        capital: u64,
        claim_id: Hash64,
        bond: PalwBondKeyV2,
        model: Option<(Hash64, u64)>,
        accepted_daa: u64,
        reservation: PalwBudgetVectorV1,
        liability_sompi: u128,
        origin: PalwBudgetOriginV1,
        admit: bool,
        mode: PalwHoldModeV1,
        j: &mut Vec<PalwBudgetWriteV1>,
    ) -> Result<(), PalwBudgetRefusalV1> {
        if self.claims.contains_key(&claim_id) {
            return Err(PalwBudgetRefusalV1::Duplicate);
        }
        let row = self.bonds.get(&bond).copied();
        if admit {
            if reservation.reward_sompi > 0 && self.has_unattributed_income_v1() {
                return Err(PalwBudgetRefusalV1::Exhausted { dim: PalwBudgetDimV1::Reward });
            }
            palw_bond_budget_admit_v1(policy, capital, row.as_ref(), &reservation, liability_sompi)?;
            self.hold_admit_v1(policy, capital, &bond, &reservation, liability_sompi, mode)?;
        }
        let reuse_not_before = accepted_daa.saturating_add(policy.window_daa);
        let mut row = row.unwrap_or_default();
        row.window = row.window.checked_add(reservation).ok_or(PalwBudgetRefusalV1::Overflow)?;
        row.open_claims = row.open_claims.saturating_add(1);
        row.open_liability_sompi = row.open_liability_sompi.saturating_add(liability_sompi);
        row.open_final_weight = row.open_final_weight.saturating_add(reservation.final_weight);
        row.latest_release_daa = row.latest_release_daa.max(reuse_not_before);
        self.put_bond(&bond, row, j);
        self.put_claim(
            &claim_id,
            Some(PalwBudgetClaimRowV1 {
                bond,
                model,
                accepted_daa,
                reuse_not_before,
                reserved: reservation,
                consumed: PalwBudgetVectorV1::ZERO,
                open: true,
                in_window: true,
                live: true,
                origin,
                liability_sompi,
            }),
            j,
        );
        self.put_release((reuse_not_before, claim_id), true, j);
        self.write_hold_shadows_v1(policy, &claim_id, &bond, accepted_daa, &reservation, origin, j)?;
        Ok(())
    }

    /// Reserve a collectible income maximum before its payment is committed.
    /// Monetary income consumes R only: no Q, B, F or execution rights, and no
    /// open-claim slot survives this atomic operation. The maximum occupies the
    /// common window even when an actual fill collects less. No fee is clipped
    /// after a seller has already transferred its asset.
    #[allow(clippy::too_many_arguments)]
    pub fn reserve_income_v1(
        &mut self,
        policy: &PalwBondBudgetPolicyV1,
        capital: u64,
        income_id: Hash64,
        bond: PalwBondKeyV2,
        accepted_daa: u64,
        maximum: u64,
        actual: u64,
        origin: PalwBudgetOriginV1,
        j: &mut Vec<PalwBudgetWriteV1>,
    ) -> Result<(), PalwBudgetRefusalV1> {
        if maximum > 0 && self.has_unattributed_income_v1() {
            return Err(PalwBudgetRefusalV1::Exhausted { dim: PalwBudgetDimV1::Reward });
        }
        if !matches!(origin, PalwBudgetOriginV1::MarketFee | PalwBudgetOriginV1::RoundFee) || actual > maximum {
            return Err(PalwBudgetRefusalV1::Assignment("invalid fee-income origin or unfunded maximum"));
        }
        if self.claims.contains_key(&income_id) {
            return Err(PalwBudgetRefusalV1::Duplicate);
        }
        if accepted_daa.checked_add(policy.window_daa).is_none() {
            return Err(PalwBudgetRefusalV1::Overflow);
        }
        let held = self.bonds.get(&bond).map_or(0, |row| row.window.reward_sompi);
        if self.bonds.get(&bond).is_some_and(|row| row.open_claims == u32::MAX) {
            return Err(PalwBudgetRefusalV1::Overflow);
        }
        let after = held.checked_add(maximum).ok_or(PalwBudgetRefusalV1::Overflow)?;
        if after > palw_bond_budget_caps_v1(policy, capital).reward_sompi {
            return Err(PalwBudgetRefusalV1::Exhausted { dim: PalwBudgetDimV1::Reward });
        }
        self.reserve(
            policy,
            capital,
            income_id,
            bond,
            None,
            accepted_daa,
            PalwBudgetVectorV1 { reward_sompi: maximum, ..PalwBudgetVectorV1::ZERO },
            origin,
            false,
            j,
        )?;
        let grant = self.consume(&income_id, PalwBudgetDimV1::Reward, u128::from(actual), true, j);
        debug_assert!(matches!(grant, Some(Ok(amount)) if amount == u128::from(actual)));
        self.close(&income_id, j);
        self.forget(&income_id, j);
        Ok(())
    }

    /// Collected market income may still wait in a low-priority native queue.
    /// Transfer its already consumed R receipt without freeing or charging R
    /// again; the physical queue drain, not collection, starts the paid window.
    /// A destination collision or overflow leaves every row and journal intact.
    pub fn queue_collected_income_v1(
        &mut self,
        income_id: &Hash64,
        queue_id: Hash64,
        j: &mut Vec<PalwBudgetWriteV1>,
    ) -> Result<(), PalwBudgetRefusalV1> {
        let row = self
            .claims
            .get(income_id)
            .copied()
            .filter(|row| {
                matches!(row.origin, PalwBudgetOriginV1::MarketFee | PalwBudgetOriginV1::RoundFee)
                    && row.in_window
                    && !row.live
                    && !row.open
                    && row.consumed.reward_sompi > 0
            })
            .ok_or(PalwBudgetRefusalV1::NotOpen)?;
        if queue_id == *income_id {
            return Err(PalwBudgetRefusalV1::Duplicate);
        }
        let old = self.claims.get(&queue_id).copied();
        if old.is_some_and(|old| {
            old.origin != PalwBudgetOriginV1::NativeIncomeQueued || old.bond != row.bond || !old.in_window || old.open
        }) {
            return Err(PalwBudgetRefusalV1::Duplicate);
        }
        let mut next = row;
        next.origin = PalwBudgetOriginV1::NativeIncomeQueued;
        next.live = true;
        if let Some(old) = old {
            next.reserved = next.reserved.checked_add(old.reserved).ok_or(PalwBudgetRefusalV1::Overflow)?;
            next.consumed = next.consumed.checked_add(old.consumed).ok_or(PalwBudgetRefusalV1::Overflow)?;
            next.accepted_daa = next.accepted_daa.min(old.accepted_daa);
            next.reuse_not_before = next.reuse_not_before.max(old.reuse_not_before);
        }
        self.put_release((row.reuse_not_before, *income_id), false, j);
        self.put_claim(income_id, None, j);
        if let Some(old) = old {
            self.put_release((old.reuse_not_before, queue_id), false, j);
        }
        self.put_claim(&queue_id, Some(next), j);
        self.put_release((next.reuse_not_before, queue_id), true, j);
        let mut bond = *self.bonds.get(&row.bond).expect("collected income bond");
        bond.latest_release_daa = u64::MAX;
        self.put_bond(&row.bond, bond, j);
        Ok(())
    }

    /// Hold the collectible maximum for an actual native recipient before
    /// accepting work. Same-bond unpaid kernel fee reservations may be moved
    /// into this hold: EXEC fees replace issuance, so reclassification must
    /// never charge the same funded right twice. A refusal writes nothing.
    #[allow(clippy::too_many_arguments)]
    pub fn hold_native_income_v1(
        &mut self,
        policy: &PalwBondBudgetPolicyV1,
        capital: u64,
        income_id: Hash64,
        bond: PalwBondKeyV2,
        accepted_daa: u64,
        liability_until: u64,
        maximum: u64,
        kernel_credits: &[Hash64],
        check_cap: bool,
        j: &mut Vec<PalwBudgetWriteV1>,
    ) -> Result<(), PalwBudgetRefusalV1> {
        let release = liability_until.checked_add(policy.window_daa).ok_or(PalwBudgetRefusalV1::Overflow)?;
        if liability_until < accepted_daa || maximum == 0 {
            return Err(PalwBudgetRefusalV1::Assignment("native income requires a funded maximum and liability"));
        }
        let old = self.claims.get(&income_id).copied();
        if old.is_some_and(|row| {
            row.bond != bond
                || row.origin != PalwBudgetOriginV1::NativeIncome
                || !row.in_window
                || row.open
                || row.consumed.reward_sompi != 0
        }) {
            return Err(PalwBudgetRefusalV1::NotOpen);
        }
        let mut credit = 0u64;
        let mut seen = BTreeSet::new();
        let mut credits = Vec::new();
        for id in kernel_credits {
            if *id == income_id || !seen.insert(*id) {
                return Err(PalwBudgetRefusalV1::Duplicate);
            }
            let row = self
                .claims
                .get(id)
                .copied()
                .filter(|row| {
                    row.bond == bond
                        && row.in_window
                        && matches!(
                            row.origin,
                            PalwBudgetOriginV1::KernelRoute
                                | PalwBudgetOriginV1::Legacy
                                | PalwBudgetOriginV1::Attempt
                                | PalwBudgetOriginV1::Rider
                                | PalwBudgetOriginV1::NativeIncome
                        )
                        && row.live
                        && row.consumed.reward_sompi == 0
                })
                .ok_or(PalwBudgetRefusalV1::NotOpen)?;
            credit = credit.checked_add(row.reserved.reward_sompi).ok_or(PalwBudgetRefusalV1::Overflow)?;
            credits.push((*id, row));
        }
        let prior = old.map_or(0, |row| row.reserved.reward_sompi);
        // A growing aggregate preserves all earlier fee maxima. The native
        // maximum supplied by the consumer is the full current work share.
        let held = prior.checked_add(credit).ok_or(PalwBudgetRefusalV1::Overflow)?.max(maximum);
        let extra = held.checked_sub(prior).and_then(|v| v.checked_sub(credit)).ok_or(PalwBudgetRefusalV1::Overflow)?;
        let mut bond_row = self.bonds.get(&bond).copied().unwrap_or_default();
        let total = bond_row.window.reward_sompi.checked_add(extra).ok_or(PalwBudgetRefusalV1::Overflow)?;
        if check_cap
            && ((extra > 0 && self.has_unattributed_income_v1()) || total > palw_bond_budget_caps_v1(policy, capital).reward_sompi)
        {
            return Err(PalwBudgetRefusalV1::Exhausted { dim: PalwBudgetDimV1::Reward });
        }
        // Preflight ends here. Reclassification leaves Q/B/F/T and open slots
        // exactly as they were, and cannot release aggregate monetary quota.
        for (id, mut row) in credits {
            if row.origin == PalwBudgetOriginV1::NativeIncome {
                // Transfer the monetary identity; no independent standalone
                // payment can remain once EXEC owns this funded right.
                self.put_release((row.reuse_not_before, id), false, j);
                self.put_claim(&id, None, j);
                self.cancel_material_pending_v1(id, j);
            } else {
                row.reserved.reward_sompi = 0;
                self.put_claim(&id, Some(row), j);
            }
        }
        bond_row.window.reward_sompi = total;
        // Pending money has no known actual payment clock. The sentinel keeps
        // the existing O(1) withdrawal gate closed across sparse DAA jumps.
        // Once all due reservations release, put_bond deletes the zero row.
        bond_row.latest_release_daa = u64::MAX;
        self.put_bond(&bond, bond_row, j);
        let mut row = old.unwrap_or(PalwBudgetClaimRowV1 {
            bond,
            model: None,
            accepted_daa,
            reuse_not_before: release,
            reserved: PalwBudgetVectorV1::ZERO,
            consumed: PalwBudgetVectorV1::ZERO,
            open: false,
            in_window: true,
            live: true,
            origin: PalwBudgetOriginV1::NativeIncome,
            // PESG §6: income reserves no liability collateral.
            liability_sompi: 0,
        });
        if let Some(old) = old {
            self.put_release((old.reuse_not_before, income_id), false, j);
        }
        row.reuse_not_before = row.reuse_not_before.max(release);
        row.live = true;
        row.reserved.reward_sompi = held;
        self.put_claim(&income_id, Some(row), j);
        self.put_release((row.reuse_not_before, income_id), true, j);
        Ok(())
    }

    /// Give every positive fund its exact queue receipt, including partial
    /// payments. Each receipt starts its W clock only when that queue actually
    /// drains. Splitting a hold never adds or releases aggregate R. The unpaid
    /// remainder cannot be transferred to another work or resurrected after
    /// cancellation; previously queued cash remains irrevocable.
    #[allow(clippy::too_many_arguments)]
    pub fn pay_native_income_v1(
        &mut self,
        policy: &PalwBondBudgetPolicyV1,
        income_id: &Hash64,
        now: u64,
        amount: u64,
        queue_id: Hash64,
        finished: bool,
        j: &mut Vec<PalwBudgetWriteV1>,
    ) -> Result<(), PalwBudgetRefusalV1> {
        if amount == 0 {
            return Err(PalwBudgetRefusalV1::Assignment("native queue credit must be positive"));
        }
        if finished {
            return self.pay_native_income_inner_v1(policy, income_id, now, amount, Some(queue_id), false, j);
        }
        let release = now.checked_add(policy.window_daa).ok_or(PalwBudgetRefusalV1::Overflow)?;
        let mut remaining = self
            .claims
            .get(income_id)
            .copied()
            .filter(|row| {
                matches!(row.origin, PalwBudgetOriginV1::NativeIncome | PalwBudgetOriginV1::NativeIncomePartial)
                    && row.live
                    && row.in_window
                    && !row.open
                    && row.consumed.reward_sompi == 0
            })
            .ok_or(PalwBudgetRefusalV1::NotOpen)?;
        if queue_id == *income_id {
            return Err(PalwBudgetRefusalV1::Duplicate);
        }
        let left =
            remaining.reserved.reward_sompi.checked_sub(amount).ok_or(PalwBudgetRefusalV1::Short { dim: PalwBudgetDimV1::Reward })?;
        let old = self.claims.get(&queue_id).copied();
        if old.is_some_and(|row| {
            row.origin != PalwBudgetOriginV1::NativeIncomeQueued || row.bond != remaining.bond || !row.in_window || row.open
        }) {
            return Err(PalwBudgetRefusalV1::Duplicate);
        }
        let mut queued = remaining;
        queued.origin = PalwBudgetOriginV1::NativeIncomeQueued;
        queued.reserved.reward_sompi = amount;
        queued.consumed.reward_sompi = amount;
        queued.reuse_not_before = queued.reuse_not_before.max(release);
        if let Some(old) = old {
            queued.reserved = queued.reserved.checked_add(old.reserved).ok_or(PalwBudgetRefusalV1::Overflow)?;
            queued.consumed = queued.consumed.checked_add(old.consumed).ok_or(PalwBudgetRefusalV1::Overflow)?;
            queued.accepted_daa = queued.accepted_daa.min(old.accepted_daa);
            queued.reuse_not_before = queued.reuse_not_before.max(old.reuse_not_before);
        }
        let material = self.material_transfer_plan_v1(*income_id, queue_id, now, amount, false)?;
        // Every destination and arithmetic check precedes the first journal write.
        remaining.origin = PalwBudgetOriginV1::NativeIncomePartial;
        remaining.reserved.reward_sompi = left;
        self.put_claim(income_id, Some(remaining), j);
        if let Some(old) = old {
            self.put_release((old.reuse_not_before, queue_id), false, j);
        }
        self.put_claim(&queue_id, Some(queued), j);
        self.put_release((queued.reuse_not_before, queue_id), true, j);
        self.apply_material_plan_v1(material, j);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn pay_native_income_inner_v1(
        &mut self,
        policy: &PalwBondBudgetPolicyV1,
        income_id: &Hash64,
        now: u64,
        amount: u64,
        queue_id: Option<Hash64>,
        source: bool,
        j: &mut Vec<PalwBudgetWriteV1>,
    ) -> Result<(), PalwBudgetRefusalV1> {
        let release = now.checked_add(policy.window_daa).ok_or(PalwBudgetRefusalV1::Overflow)?;
        let row = self
            .claims
            .get(income_id)
            .copied()
            .filter(|row| {
                (matches!(row.origin, PalwBudgetOriginV1::NativeIncome | PalwBudgetOriginV1::NativeIncomePartial)
                    || (source && row.origin == PalwBudgetOriginV1::SourceFee))
                    && row.live
                    && row.in_window
                    && !row.open
            })
            .ok_or(PalwBudgetRefusalV1::NotOpen)?;
        let paid = row.consumed.reward_sompi.checked_add(amount).ok_or(PalwBudgetRefusalV1::Overflow)?;
        if paid > row.reserved.reward_sompi {
            return Err(PalwBudgetRefusalV1::Short { dim: PalwBudgetDimV1::Reward });
        }
        let queued = match queue_id {
            Some(id) if id == *income_id => return Err(PalwBudgetRefusalV1::Duplicate),
            Some(id) => {
                let old = self.claims.get(&id).copied();
                if old.is_some_and(|other| {
                    other.origin != PalwBudgetOriginV1::NativeIncomeQueued || other.bond != row.bond || !other.in_window || other.open
                }) {
                    return Err(PalwBudgetRefusalV1::Duplicate);
                }
                let mut next = row;
                next.origin = PalwBudgetOriginV1::NativeIncomeQueued;
                next.consumed.reward_sompi = paid;
                next.reuse_not_before = next.reuse_not_before.max(release);
                if let Some(old) = old {
                    next.reserved = next.reserved.checked_add(old.reserved).ok_or(PalwBudgetRefusalV1::Overflow)?;
                    next.consumed = next.consumed.checked_add(old.consumed).ok_or(PalwBudgetRefusalV1::Overflow)?;
                    next.reuse_not_before = next.reuse_not_before.max(old.reuse_not_before);
                    next.accepted_daa = next.accepted_daa.min(old.accepted_daa);
                }
                Some((id, old, next))
            }
            None => None,
        };
        let material = match queue_id {
            Some(id) => self.material_transfer_plan_v1(*income_id, id, now, amount, true)?,
            None => Vec::new(),
        };
        if let Some((id, old, next)) = queued {
            self.put_release((row.reuse_not_before, *income_id), false, j);
            self.put_claim(income_id, None, j);
            if let Some(old) = old {
                self.put_release((old.reuse_not_before, id), false, j);
            }
            self.put_claim(&id, Some(next), j);
            self.put_release((next.reuse_not_before, id), true, j);
            let mut bond = *self.bonds.get(&row.bond).expect("native income bond");
            bond.latest_release_daa = bond.latest_release_daa.max(next.reuse_not_before);
            self.put_bond(&row.bond, bond, j);
        } else {
            self.extend_income_window_v1(income_id, release, j);
            let mut row = *self.claims.get(income_id).expect("extended native hold");
            row.consumed.reward_sompi = paid;
            self.put_claim(income_id, Some(row), j);
        }
        self.apply_material_plan_v1(material, j);
        Ok(())
    }

    /// Record a funded court award without making recipient capacity a
    /// condition of conviction. The immutable total is preserved even when it
    /// exceeds one R window; consumed is exact irrevocably queued progress.
    /// A repeated receipt is idempotent, and cannot change its amount/payee.
    pub fn stage_reporter_income_v1(
        &mut self,
        award: Hash64,
        bond: PalwBondKeyV2,
        now: u64,
        amount: u64,
        j: &mut Vec<PalwBudgetWriteV1>,
    ) -> Result<(), PalwBudgetRefusalV1> {
        if amount == 0 {
            return Ok(());
        }
        if let Some(old) = self.claims.get(&award) {
            return if old.origin == PalwBudgetOriginV1::DeferredReporterIncome
                && old.bond == bond
                && old.accepted_daa == now
                && old.reserved.reward_sompi == amount
            {
                Ok(())
            } else {
                Err(PalwBudgetRefusalV1::Duplicate)
            };
        }
        self.put_claim(
            &award,
            Some(PalwBudgetClaimRowV1 {
                bond,
                model: None,
                accepted_daa: now,
                reuse_not_before: 0,
                reserved: PalwBudgetVectorV1 { reward_sompi: amount, ..PalwBudgetVectorV1::ZERO },
                consumed: PalwBudgetVectorV1::ZERO,
                origin: PalwBudgetOriginV1::DeferredReporterIncome,
                open: false,
                live: true,
                in_window: false,
                liability_sompi: 0,
            }),
            j,
        );
        Ok(())
    }

    /// Redeem an exact funded tranche into a unique physical payout receipt.
    /// No Q/B/F/T credit is created. Zero capacity is a reversible deferral,
    /// and every positive tranche starts W only at actual coinbase payment.
    #[allow(clippy::too_many_arguments)]
    pub fn redeem_reporter_income_v1(
        &mut self,
        policy: &PalwBondBudgetPolicyV1,
        capital: u64,
        award: &Hash64,
        now: u64,
        queue_id: Hash64,
        maximum_tranche: u64,
        j: &mut Vec<PalwBudgetWriteV1>,
    ) -> Result<u64, PalwBudgetRefusalV1> {
        let mut debt = self
            .claims
            .get(award)
            .copied()
            .filter(|row| row.origin == PalwBudgetOriginV1::DeferredReporterIncome && row.live && !row.open && !row.in_window)
            .ok_or(PalwBudgetRefusalV1::NotOpen)?;
        let mut bond = self.bonds.get(&debt.bond).copied().unwrap_or_default();
        let remaining = debt.reserved.reward_sompi.checked_sub(debt.consumed.reward_sompi).ok_or(PalwBudgetRefusalV1::Overflow)?;
        let capacity = if self.has_unattributed_income_v1() {
            0
        } else {
            palw_bond_budget_caps_v1(policy, capital).reward_sompi.saturating_sub(bond.window.reward_sompi)
        };
        let amount = remaining.min(capacity).min(maximum_tranche);
        if amount == 0 {
            return Ok(0);
        }
        if self.claims.contains_key(&queue_id) {
            return Err(PalwBudgetRefusalV1::Duplicate);
        }
        let release = now.checked_add(policy.window_daa).ok_or(PalwBudgetRefusalV1::Overflow)?;
        let vector = PalwBudgetVectorV1 { reward_sompi: amount, ..PalwBudgetVectorV1::ZERO };
        bond.window = bond.window.checked_add(vector).ok_or(PalwBudgetRefusalV1::Overflow)?;
        bond.latest_release_daa = u64::MAX;
        let material = self.material_transfer_plan_v1(*award, queue_id, now, amount, amount == remaining)?;
        debt.consumed.reward_sompi += amount; // bounded by immutable remaining above
        self.put_claim(award, Some(debt), j);
        self.put_claim(
            &queue_id,
            Some(PalwBudgetClaimRowV1 {
                bond: debt.bond,
                model: None,
                accepted_daa: now,
                reuse_not_before: release,
                reserved: vector,
                consumed: vector,
                origin: PalwBudgetOriginV1::NativeIncomeQueued,
                open: false,
                live: true,
                in_window: true,
                liability_sompi: 0,
            }),
            j,
        );
        self.put_release((release, queue_id), true, j);
        self.put_bond(&debt.bond, bond, j);
        self.apply_material_plan_v1(material, j);
        Ok(amount)
    }

    /// The native fold drained the actual queue credit into this block's
    /// validated coinbase. Sparse DAA jumps use this actual payment clock.
    pub fn confirm_native_income_payment_v1(
        &mut self,
        policy: &PalwBondBudgetPolicyV1,
        income_id: &Hash64,
        now: u64,
        j: &mut Vec<PalwBudgetWriteV1>,
    ) -> Result<(), PalwBudgetRefusalV1> {
        let release = now.checked_add(policy.window_daa).ok_or(PalwBudgetRefusalV1::Overflow)?;
        self.claims
            .get(income_id)
            .filter(|row| row.origin == PalwBudgetOriginV1::NativeIncomeQueued && row.live)
            .ok_or(PalwBudgetRefusalV1::NotOpen)?;
        let material = self.material_payment_plan_v1(*income_id, now, policy.window_daa)?;
        self.extend_income_window_v1(income_id, release, j);
        self.forget(income_id, j);
        self.apply_material_plan_v1(material, j);
        Ok(())
    }

    /// Cancel only the unpaid remainder after void/forfeiture. Already paid
    /// income and its window remain; cancellation never recovers quota early.
    pub fn cancel_native_income_v1(&mut self, income_id: &Hash64, j: &mut Vec<PalwBudgetWriteV1>) -> Result<(), PalwBudgetRefusalV1> {
        self.claims
            .get(income_id)
            .filter(|row| {
                matches!(row.origin, PalwBudgetOriginV1::NativeIncome | PalwBudgetOriginV1::NativeIncomePartial)
                    && row.live
                    && !row.open
            })
            .ok_or(PalwBudgetRefusalV1::NotOpen)?;
        self.forget(income_id, j);
        self.cancel_material_pending_v1(*income_id, j);
        Ok(())
    }

    /// Reserve a fixed, authenticated source-fee promise before its opt-in.
    /// The monetary right creates no open computation, block or weight credit.
    /// Its R remains occupied while unpaid and at least through H + W.
    #[allow(clippy::too_many_arguments)]
    pub fn reserve_source_income_v1(
        &mut self,
        policy: &PalwBondBudgetPolicyV1,
        capital: u64,
        income_id: Hash64,
        bond: PalwBondKeyV2,
        accepted_daa: u64,
        liability_until: u64,
        fee: u64,
        j: &mut Vec<PalwBudgetWriteV1>,
    ) -> Result<(), PalwBudgetRefusalV1> {
        self.hold_source_income_v1(policy, capital, income_id, bond, accepted_daa, liability_until, fee, true, j)
    }

    /// Migrate an authenticated pre-fence fee promise. Existing signed rights
    /// are counted even when they fill or exceed the new R cap; no future
    /// admission may use that occupied room. Pending income survives old W/H.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn seed_source_income_v1(
        &mut self,
        policy: &PalwBondBudgetPolicyV1,
        capital: u64,
        income_id: Hash64,
        bond: PalwBondKeyV2,
        accepted_daa: u64,
        liability_until: u64,
        fee: u64,
        j: &mut Vec<PalwBudgetWriteV1>,
    ) -> Result<(), PalwBudgetRefusalV1> {
        self.hold_source_income_v1(policy, capital, income_id, bond, accepted_daa, liability_until, fee, false, j)
    }

    #[allow(clippy::too_many_arguments)]
    fn hold_source_income_v1(
        &mut self,
        policy: &PalwBondBudgetPolicyV1,
        capital: u64,
        income_id: Hash64,
        bond: PalwBondKeyV2,
        accepted_daa: u64,
        liability_until: u64,
        fee: u64,
        check_cap: bool,
        j: &mut Vec<PalwBudgetWriteV1>,
    ) -> Result<(), PalwBudgetRefusalV1> {
        if fee == 0 || liability_until < accepted_daa {
            return Err(PalwBudgetRefusalV1::Assignment("source income requires a funded fee and a future liability"));
        }
        let release = liability_until.checked_add(policy.window_daa).ok_or(PalwBudgetRefusalV1::Overflow)?;
        if self.claims.contains_key(&income_id) {
            return Err(PalwBudgetRefusalV1::Duplicate);
        }
        let row = self.bonds.get(&bond).copied().unwrap_or_default();
        if row.open_claims == u32::MAX {
            return Err(PalwBudgetRefusalV1::Overflow);
        }
        let held = row.window.reward_sompi.checked_add(fee).ok_or(PalwBudgetRefusalV1::Overflow)?;
        if check_cap && (self.has_unattributed_income_v1() || held > palw_bond_budget_caps_v1(policy, capital).reward_sompi) {
            return Err(PalwBudgetRefusalV1::Exhausted { dim: PalwBudgetDimV1::Reward });
        }
        self.reserve(
            policy,
            capital,
            income_id,
            bond,
            None,
            accepted_daa,
            PalwBudgetVectorV1 { reward_sompi: fee, ..PalwBudgetVectorV1::ZERO },
            PalwBudgetOriginV1::SourceFee,
            false,
            j,
        )?;
        self.close(&income_id, j);
        self.extend_income_window_v1(&income_id, release, j);
        let mut row = *self.bonds.get(&bond).expect("pending source income bond");
        row.latest_release_daa = u64::MAX;
        self.put_bond(&bond, row, j);
        Ok(())
    }

    fn extend_income_window_v1(&mut self, income_id: &Hash64, release: u64, j: &mut Vec<PalwBudgetWriteV1>) {
        let mut claim = *self.claims.get(income_id).expect("a checked income row");
        if release > claim.reuse_not_before {
            self.put_release((claim.reuse_not_before, *income_id), false, j);
            claim.reuse_not_before = release;
            self.put_claim(income_id, Some(claim), j);
            self.put_release((release, *income_id), true, j);
            let mut bond = self.bonds.get(&claim.bond).copied().expect("an in-window income bond");
            bond.latest_release_daa = bond.latest_release_daa.max(release);
            self.put_bond(&claim.bond, bond, j);
        }
    }

    /// Pay exactly the promised fixed source fee, once, strictly after its
    /// inclusive liability H. The fee stays in R for W after actual payment.
    pub fn settle_source_income_v1(
        &mut self,
        policy: &PalwBondBudgetPolicyV1,
        income_id: &Hash64,
        liability_until: u64,
        now: u64,
        fee: u64,
        j: &mut Vec<PalwBudgetWriteV1>,
    ) -> Result<(), PalwBudgetRefusalV1> {
        let release = now.checked_add(policy.window_daa).ok_or(PalwBudgetRefusalV1::Overflow)?;
        let mut claim = self
            .claims
            .get(income_id)
            .copied()
            .filter(|claim| claim.origin == PalwBudgetOriginV1::SourceFee && claim.live && claim.in_window && !claim.open)
            .ok_or(PalwBudgetRefusalV1::NotOpen)?;
        if now <= liability_until || fee == 0 || fee != claim.reserved.reward_sompi || claim.consumed.reward_sompi != 0 {
            return Err(PalwBudgetRefusalV1::Assignment("source fee has not matured or differs from its funded promise"));
        }
        if self.material_income.contains_key(&material_pending_id_v1(*income_id)) {
            return Err(PalwBudgetRefusalV1::Assignment("rooted material source income requires its actual native queue"));
        }
        self.extend_income_window_v1(income_id, release, j);
        claim = *self.claims.get(income_id).expect("extended checked income");
        claim.consumed.reward_sompi = fee;
        self.put_claim(income_id, Some(claim), j);
        self.forget(income_id, j);
        Ok(())
    }

    /// A signed source promise became an irrevocable native queue credit.
    /// Logical `Paid` is not yet a coinbase payment: retain the same maximum
    /// until the common queue drain confirms actual payment + W. Preflight
    /// includes the destination aggregate, so any refusal writes nothing.
    #[allow(clippy::too_many_arguments)]
    pub fn queue_source_income_v1(
        &mut self,
        policy: &PalwBondBudgetPolicyV1,
        income_id: &Hash64,
        liability_until: u64,
        now: u64,
        fee: u64,
        queue_id: Hash64,
        j: &mut Vec<PalwBudgetWriteV1>,
    ) -> Result<(), PalwBudgetRefusalV1> {
        let claim = self
            .claims
            .get(income_id)
            .filter(|row| row.origin == PalwBudgetOriginV1::SourceFee && row.live && row.in_window && !row.open)
            .ok_or(PalwBudgetRefusalV1::NotOpen)?;
        if now <= liability_until || fee == 0 || fee != claim.reserved.reward_sompi || claim.consumed.reward_sompi != 0 {
            return Err(PalwBudgetRefusalV1::Assignment("source queue credit has not matured or differs from its funded promise"));
        }
        self.pay_native_income_inner_v1(policy, income_id, now, fee, Some(queue_id), true, j)
    }

    /// Cancel the unpaid right after its native funding is refunded. Window
    /// occupancy is retained; a refund is not a quota recovery.
    pub fn cancel_source_income_v1(&mut self, income_id: &Hash64, j: &mut Vec<PalwBudgetWriteV1>) -> Result<(), PalwBudgetRefusalV1> {
        let row = self
            .claims
            .get(income_id)
            .filter(|claim| claim.origin == PalwBudgetOriginV1::SourceFee && claim.live && claim.consumed.reward_sompi == 0)
            .ok_or(PalwBudgetRefusalV1::NotOpen)?;
        debug_assert!(!row.open && row.in_window);
        self.forget(income_id, j);
        self.cancel_material_pending_v1(*income_id, j);
        Ok(())
    }

    /// **A claim's whole acceptance step** (design §2.5, §3.5), atomic: the reservation from the ask (per-claim ceilings), its reward
    /// clipped to the model's available budget when `model` names one in the current epoch, the bond's admission, then the writes
    /// (the model's reservation with the claim's). Returns the reservation taken. Nothing is written on a refusal.
    #[allow(clippy::too_many_arguments)]
    pub fn reserve_claim_v1(
        &mut self,
        policy: &PalwBondBudgetPolicyV1,
        capital: u64,
        claim_id: Hash64,
        bond: PalwBondKeyV2,
        model: Option<Hash64>,
        accepted_daa: u64,
        ask: PalwBudgetAskV1,
        origin: PalwBudgetOriginV1,
        j: &mut Vec<PalwBudgetWriteV1>,
    ) -> Result<PalwBudgetVectorV1, PalwBudgetRefusalV1> {
        self.reserve_claim_moded_v1(
            policy,
            capital,
            claim_id,
            bond,
            model,
            accepted_daa,
            ask,
            origin,
            PalwHoldModeV1::for_origin(origin),
            j,
        )
    }

    /// [`Self::reserve_claim_v1`] with the claim's verification mode (ユーザー方針 2026-10-15).
    #[allow(clippy::too_many_arguments)]
    pub fn reserve_claim_moded_v1(
        &mut self,
        policy: &PalwBondBudgetPolicyV1,
        capital: u64,
        claim_id: Hash64,
        bond: PalwBondKeyV2,
        model: Option<Hash64>,
        accepted_daa: u64,
        ask: PalwBudgetAskV1,
        origin: PalwBudgetOriginV1,
        mode: PalwHoldModeV1,
        j: &mut Vec<PalwBudgetWriteV1>,
    ) -> Result<PalwBudgetVectorV1, PalwBudgetRefusalV1> {
        if self.claims.contains_key(&claim_id) {
            return Err(PalwBudgetRefusalV1::Duplicate);
        }
        let reservation = self.preview_claim_for_origin_moded_v1(policy, capital, bond, model, ask, origin, mode)?;
        // RFC-0016 P4: a user-paid escrow claim takes nothing from (and records nothing against) the coinbase-derived model budget.
        let model = match (model.filter(|_| !palw_budget_origin_is_user_escrow_v1(origin)), self.header.allocation) {
            (Some(m), Some(epoch)) => Some((m, epoch.index)),
            _ => None,
        };
        if let Some((m, _)) = model
            && reservation.reward_sompi > 0
        {
            let granted = self.reserve_model_reward(&m, reservation.reward_sompi, j);
            debug_assert_eq!(granted, reservation.reward_sompi, "availability was read above in the same state");
        }
        self.reserve_liable_moded_v1(
            policy,
            capital,
            claim_id,
            bond,
            model,
            accepted_daa,
            reservation,
            ask.liability_sompi,
            origin,
            false,
            mode,
            j,
        )?;
        Ok(reservation)
    }

    /// **The reservation [`Self::reserve_claim_v1`] would take, and its admission — without writing** (a path plans before its first
    /// write and commits after).
    pub fn preview_claim_v1(
        &self,
        policy: &PalwBondBudgetPolicyV1,
        capital: u64,
        bond: PalwBondKeyV2,
        model: Option<Hash64>,
        ask: PalwBudgetAskV1,
    ) -> Result<PalwBudgetVectorV1, PalwBudgetRefusalV1> {
        self.preview_claim_moded_v1(policy, capital, bond, model, ask, PalwHoldModeV1::PanelOne)
    }

    /// [`Self::preview_claim_v1`] with the claim's verification mode: the hold admission (day view for Panel=1, burst view, concurrent
    /// liability) runs after the long-window admission, before any write.
    pub fn preview_claim_moded_v1(
        &self,
        policy: &PalwBondBudgetPolicyV1,
        capital: u64,
        bond: PalwBondKeyV2,
        model: Option<Hash64>,
        ask: PalwBudgetAskV1,
        mode: PalwHoldModeV1,
    ) -> Result<PalwBudgetVectorV1, PalwBudgetRefusalV1> {
        let mut reservation = palw_bond_budget_reservation_v1(policy, ask);
        if let (Some(m), Some(_)) = (model, self.header.allocation) {
            reservation.reward_sompi = reservation.reward_sompi.min(self.model_available(&m));
        }
        if reservation.reward_sompi > 0 && self.has_unattributed_income_v1() {
            return Err(PalwBudgetRefusalV1::Exhausted { dim: PalwBudgetDimV1::Reward });
        }
        palw_bond_budget_admit_v1(policy, capital, self.bonds.get(&bond), &reservation, ask.liability_sompi)?;
        self.hold_admit_v1(policy, capital, &bond, &reservation, ask.liability_sompi, mode)?;
        Ok(reservation)
    }

    /// **[`Self::preview_claim_v1`] for a claim of `origin`** (RFC-0016 P4, user decision 2026-10-15). A coinbase-derived origin is
    /// [`Self::preview_claim_v1`] unchanged. A user-paid escrow origin ([`palw_budget_origin_is_user_escrow_v1`]):
    ///
    /// * is NEVER clipped by the model-allocation budget (`model` is ignored: that budget is coinbase-derived);
    /// * must reserve its WHOLE reward: a per-claim ceiling below the escrow is [`PalwBudgetRefusalV1::EscrowNotReservable`], and a
    ///   bond whose common `R` cap cannot hold it is `Exhausted { Reward }` — both before acceptance, never a clip after.
    pub fn preview_claim_for_origin_v1(
        &self,
        policy: &PalwBondBudgetPolicyV1,
        capital: u64,
        bond: PalwBondKeyV2,
        model: Option<Hash64>,
        ask: PalwBudgetAskV1,
        origin: PalwBudgetOriginV1,
    ) -> Result<PalwBudgetVectorV1, PalwBudgetRefusalV1> {
        self.preview_claim_for_origin_moded_v1(policy, capital, bond, model, ask, origin, PalwHoldModeV1::for_origin(origin))
    }

    /// [`Self::preview_claim_for_origin_v1`] with the claim's verification mode (ユーザー方針 2026-10-15): a coinbase-derived origin is
    /// [`Self::preview_claim_moded_v1`]; a user-paid escrow origin keeps RFC-0016 P4's rules and runs the same hold admission (day view
    /// for Panel=1, burst view, concurrent liability).
    #[allow(clippy::too_many_arguments)]
    pub fn preview_claim_for_origin_moded_v1(
        &self,
        policy: &PalwBondBudgetPolicyV1,
        capital: u64,
        bond: PalwBondKeyV2,
        model: Option<Hash64>,
        ask: PalwBudgetAskV1,
        origin: PalwBudgetOriginV1,
        mode: PalwHoldModeV1,
    ) -> Result<PalwBudgetVectorV1, PalwBudgetRefusalV1> {
        if !palw_budget_origin_is_user_escrow_v1(origin) {
            return self.preview_claim_moded_v1(policy, capital, bond, model, ask, mode);
        }
        let reservation = palw_bond_budget_reservation_v1(policy, ask);
        if reservation.reward_sompi < ask.reward_sompi {
            return Err(PalwBudgetRefusalV1::EscrowNotReservable);
        }
        if reservation.reward_sompi > 0 && self.has_unattributed_income_v1() {
            return Err(PalwBudgetRefusalV1::Exhausted { dim: PalwBudgetDimV1::Reward });
        }
        palw_bond_budget_admit_v1(policy, capital, self.bonds.get(&bond), &reservation, ask.liability_sompi)?;
        self.hold_admit_v1(policy, capital, &bond, &reservation, ask.liability_sompi, mode)?;
        Ok(reservation)
    }

    /// **Move `units` of a lead's consumed block to its riders** (design §2.1): the lead's reserved and consumed block units, and its
    /// bond's window, fall by `units` — which the riders' own reservations take in the same transition, so the window keeps one block.
    pub fn transfer_block_units_v1(
        &mut self,
        lead: &Hash64,
        units: u64,
        j: &mut Vec<PalwBudgetWriteV1>,
    ) -> Result<(), PalwBudgetRefusalV1> {
        let mut claim = self.claims.get(lead).copied().filter(|c| c.open && c.in_window).ok_or(PalwBudgetRefusalV1::NotOpen)?;
        if claim.consumed.block_units < units || claim.reserved.block_units < units {
            return Err(PalwBudgetRefusalV1::Short { dim: PalwBudgetDimV1::BlockUnits });
        }
        claim.consumed.block_units -= units;
        claim.reserved.block_units -= units;
        let mut bond = self.bonds.get(&claim.bond).copied().unwrap_or_default();
        bond.window.block_units = bond.window.block_units.checked_sub(units).ok_or(PalwBudgetRefusalV1::Overflow)?;
        self.put_bond(&claim.bond.clone(), bond, j);
        self.put_claim(lead, Some(claim), j);
        // The riders' reservations write their own shadows: the lead's shadows give the moved units back (same block).
        self.shrink_hold_shadows_v1(lead, &PalwBudgetVectorV1 { block_units: units, ..PalwBudgetVectorV1::ZERO }, j);
        Ok(())
    }

    /// **Shrink a reservation in the block that took it** (riders re-attribute their lead's block and escrow, design §2.1): every
    /// component may only fall and never below what was consumed. Not a recovery: it runs only at the acceptance DAA.
    pub fn shrink_same_block(
        &mut self,
        claim_id: &Hash64,
        new_reserved: PalwBudgetVectorV1,
        now: u64,
        j: &mut Vec<PalwBudgetWriteV1>,
    ) -> Result<(), PalwBudgetRefusalV1> {
        let mut claim = self.claims.get(claim_id).copied().filter(|c| c.open && c.in_window).ok_or(PalwBudgetRefusalV1::NotOpen)?;
        if claim.accepted_daa != now || !new_reserved.fits_within(&claim.reserved) || !claim.consumed.fits_within(&new_reserved) {
            return Err(PalwBudgetRefusalV1::BadShrink);
        }
        let freed = claim.reserved.checked_sub(new_reserved).ok_or(PalwBudgetRefusalV1::BadShrink)?;
        let mut bond = self.bonds.get(&claim.bond).copied().unwrap_or_default();
        bond.window = bond.window.checked_sub(freed).ok_or(PalwBudgetRefusalV1::Overflow)?;
        bond.open_final_weight = bond.open_final_weight.checked_sub(freed.final_weight).ok_or(PalwBudgetRefusalV1::Overflow)?;
        self.put_bond(&claim.bond.clone(), bond, j);
        // A model reservation follows its claim's reward down (same epoch, same block).
        if let Some((model, epoch)) = claim.model
            && freed.reward_sompi > 0
            && let Some(mut row) = self.models.get(&model).copied().filter(|row| row.epoch == epoch)
        {
            row.reserved_sompi = row.reserved_sompi.saturating_sub(freed.reward_sompi);
            put(&mut self.models, PALW_BUDGET_TABLE_MODELS_V1, &model, Some(row), j);
        }
        claim.reserved = new_reserved;
        self.put_claim(claim_id, Some(claim), j);
        self.shrink_hold_shadows_v1(claim_id, &freed, j);
        Ok(())
    }

    /// **Consume from a claim's own reservation** (design §2.6): `None` when the claim has no budget row or a Legacy one (the caller runs
    /// the old rule unchanged); else `Ok(grant)` with `grant = min(ask, reserved − consumed)` — `Err(Short)` instead when `strict` and the
    /// grant would be less than the ask (a reward block whose worker share is paid whole). `Err(NotOpen)` for a closed claim.
    pub fn consume(
        &mut self,
        claim_id: &Hash64,
        dim: PalwBudgetDimV1,
        ask: u128,
        strict: bool,
        j: &mut Vec<PalwBudgetWriteV1>,
    ) -> Option<Result<u128, PalwBudgetRefusalV1>> {
        let mut claim = self.claims.get(claim_id).copied()?;
        if claim.origin == PalwBudgetOriginV1::Legacy {
            return None;
        }
        if !claim.open {
            return Some(Err(PalwBudgetRefusalV1::NotOpen));
        }
        let left = claim.reserved.get(dim).saturating_sub(claim.consumed.get(dim));
        let grant = ask.min(left);
        if strict && grant < ask {
            return Some(Err(PalwBudgetRefusalV1::Short { dim }));
        }
        match dim {
            PalwBudgetDimV1::Claims => claim.consumed.claims += grant as u64,
            PalwBudgetDimV1::BlockUnits => claim.consumed.block_units += grant as u64,
            PalwBudgetDimV1::Reward => claim.consumed.reward_sompi += grant as u64,
            PalwBudgetDimV1::FinalWeight => claim.consumed.final_weight += grant,
            PalwBudgetDimV1::RoundRights => claim.consumed.round_rights += grant as u64,
        }
        if grant > 0 {
            self.put_claim(claim_id, Some(claim), j);
        }
        Some(Ok(grant))
    }

    /// **What a claim may still be granted** in `dim` without consuming (`None`: no row or a Legacy row — the old rule applies).
    pub fn remaining(&self, claim_id: &Hash64, dim: PalwBudgetDimV1) -> Option<u128> {
        let claim = self.claims.get(claim_id).filter(|c| c.origin != PalwBudgetOriginV1::Legacy)?;
        Some(if claim.open { claim.reserved.get(dim).saturating_sub(claim.consumed.get(dim)) } else { 0 })
    }

    /// **Close a claim** (Final, void, conviction; design §2.7): no further consumption, one fewer open claim. Its reservation stays in
    /// the window until `reuse_not_before` — closing returns no room. Idempotent.
    pub fn close(&mut self, claim_id: &Hash64, j: &mut Vec<PalwBudgetWriteV1>) {
        self.close_at(claim_id, None, j);
    }

    /// [`Self::close`], starting the bond's liability hold until `hold_until` (PESG §6: terminal + `H_L`; `None` for a row that is
    /// no claim). The hold only ever extends.
    pub fn close_at(&mut self, claim_id: &Hash64, hold_until: Option<u64>, j: &mut Vec<PalwBudgetWriteV1>) {
        let Some(mut claim) = self.claims.get(claim_id).copied().filter(|c| c.open) else { return };
        let mut bond = self.bonds.get(&claim.bond).copied().unwrap_or_default();
        bond.open_claims = bond.open_claims.saturating_sub(1);
        bond.open_liability_sompi = bond.open_liability_sompi.saturating_sub(claim.liability_sompi);
        bond.open_final_weight = bond.open_final_weight.saturating_sub(claim.reserved.final_weight);
        if let Some(until) = hold_until.filter(|until| *until > bond.liability_hold_until) {
            if bond.liability_hold_until > 0 {
                self.put_liability_release((bond.liability_hold_until, claim.bond), false, j);
            }
            bond.liability_hold_until = until;
            self.put_liability_release((until, claim.bond), true, j);
        }
        self.put_bond(&claim.bond.clone(), bond, j);
        claim.open = false;
        self.put_claim(claim_id, Some(claim), j);
    }

    /// **The V2 claim left the state** (retirement): close it if still open, and drop its row once it is out of the window too.
    pub fn forget(&mut self, claim_id: &Hash64, j: &mut Vec<PalwBudgetWriteV1>) {
        // Ordinary claim retirement cannot erase immutable funded reporter debt
        // or its completed receipt (which prevents award reclassification).
        if self.claims.get(claim_id).is_some_and(|row| row.origin == PalwBudgetOriginV1::DeferredReporterIncome) {
            return;
        }
        self.close(claim_id, j);
        let Some(mut claim) = self.claims.get(claim_id).copied().filter(|c| c.live) else { return };
        claim.live = false;
        self.put_claim(claim_id, claim.in_window.then_some(claim), j);
    }

    /// **What a budgeted claim was granted in Final weight** (hook H-4) — `None` for a claim with no row or a Legacy one.
    pub fn final_weight_granted(&self, claim_id: &Hash64) -> Option<u128> {
        self.claims.get(claim_id).filter(|c| c.origin != PalwBudgetOriginV1::Legacy).map(|c| c.consumed.final_weight)
    }

    /// **The engine's own invariants** (tests and the state's consistency check): every bond's window is exactly the sum of its
    /// in-window reservations and its open count the number of its open rows; every in-window row has its release and no other release
    /// exists; consumption within reservation; a model's reservations this epoch within its availability base.
    pub fn check_consistency(&self) -> Result<(), &'static str> {
        self.material_consistent_v1()?;
        self.consumer_consistent_v1()?;
        if !matches!(
            self.header.version,
            PALW_BOND_BUDGET_STATE_VERSION_V1
                | PALW_BOND_BUDGET_STATE_VERSION_V2
                | PALW_BOND_BUDGET_STATE_VERSION_V3
                | PALW_BOND_BUDGET_STATE_VERSION_V4
        ) || (self.header.version == PALW_BOND_BUDGET_STATE_VERSION_V1 && !self.reporter_income.is_empty())
        {
            return Err("invalid reporter monetary state version");
        }
        if self.unattributed_income != self.reporter_income.values().filter(|row| row.recipient.is_none()).count() {
            return Err("the unattributed monetary index disagrees with rooted liabilities");
        }
        let mut pending_reporters = BTreeMap::new();
        for row in self.reporter_income.values() {
            if row.awarded_daa.is_none()
                && let Some(bond) = row.recipient
            {
                *pending_reporters.entry(bond).or_insert(0usize) += 1;
            }
            if !row.valid() {
                return Err("invalid reporter income binding or legacy physical queue liability");
            }
        }
        if pending_reporters != self.pending_reporters {
            return Err("the pending reporter withdrawal index disagrees with rooted bindings");
        }
        let mut windows: BTreeMap<PalwBondKeyV2, PalwBudgetBondRowV1> = BTreeMap::new();
        let mut deferred = BTreeMap::new();
        let mut awards = BTreeSet::new();
        for (id, claim) in &self.claims {
            if claim.origin == PalwBudgetOriginV1::DeferredReporterIncome {
                if claim.open
                    || claim.in_window
                    || !claim.live
                    || claim.model.is_some()
                    || claim.reuse_not_before != 0
                    || claim.reserved.reward_sompi == 0
                    || claim.reserved != (PalwBudgetVectorV1 { reward_sompi: claim.reserved.reward_sompi, ..PalwBudgetVectorV1::ZERO })
                    || claim.consumed != (PalwBudgetVectorV1 { reward_sompi: claim.consumed.reward_sompi, ..PalwBudgetVectorV1::ZERO })
                {
                    return Err("a deferred reporter award violates its immutable monetary schema");
                }
                if Self::unpaid_reporter_debt(claim) {
                    *deferred.entry(claim.bond).or_insert(0usize) += 1;
                    awards.insert(*id);
                }
            }
            if !claim.live && !claim.in_window {
                return Err("a row that is neither live nor in the window was kept");
            }
            if !claim.consumed.fits_within(&claim.reserved) {
                return Err("a claim consumed beyond its reservation");
            }
            if claim.in_window != self.releases.contains(&(claim.reuse_not_before, *id)) {
                return Err("an in-window row without its release, or a release of a row out of the window");
            }
            let row = windows.entry(claim.bond).or_default();
            if claim.in_window {
                row.window = row.window.checked_add(claim.reserved).ok_or("window overflow")?;
                row.latest_release_daa = row.latest_release_daa.max(claim.reuse_not_before);
            }
            if claim.open {
                row.open_claims += 1;
                row.open_liability_sompi += claim.liability_sompi;
                row.open_final_weight += claim.reserved.final_weight;
            }
        }
        if deferred != self.deferred_by_bond || awards != self.deferred_awards {
            return Err("the deferred reporter withdrawal index disagrees with its rooted debt");
        }
        if self.releases.iter().any(|(_, id)| !self.claims.contains_key(id)) {
            return Err("a release names no row");
        }
        // PESG §6: every liability hold has exactly one release entry, and every entry is its bond's current hold.
        for (bond, row) in &self.bonds {
            if row.liability_hold_until > 0 && !self.liability_releases.contains(&(row.liability_hold_until, *bond)) {
                return Err("a liability hold without its release");
            }
            if row.liability_hold_until > 0 {
                windows.entry(*bond).or_default().liability_hold_until = row.liability_hold_until;
            }
        }
        if self.liability_releases.iter().any(|(at, bond)| self.bonds.get(bond).is_none_or(|row| row.liability_hold_until != *at)) {
            return Err("a liability release that is not its bond's hold");
        }
        windows.retain(|_, row| !row.window.is_zero() || row.open_claims > 0 || row.liability_hold_until > 0);
        if windows.len() != self.bonds.len() {
            return Err("the bond table does not match the claims");
        }
        for (bond, expected) in windows {
            let row = self.bonds.get(&bond).ok_or("a bond with claims has no row")?;
            if row.window != expected.window
                || row.open_claims != expected.open_claims
                || row.open_liability_sompi != expected.open_liability_sompi
                || row.open_final_weight != expected.open_final_weight
                || row.latest_release_daa < expected.latest_release_daa
            {
                return Err("a bond's window, open count, open liability or unsettled weight disagrees with its claims");
            }
        }
        Ok(())
    }

    /// **Reserve a bond's Round rights drawn in `span`** (readiness §3e): `n` allocated tickets, reserved and used at once (an allocated
    /// ticket is a used right, executed or not), released only at `draw_daa + W`. The caller capped the candidates at
    /// [`palw_round_rights_remaining_v1`], so this fits; it is admitted against the caps all the same.
    pub fn reserve_round_rights_v1(
        &mut self,
        policy: &PalwBondBudgetPolicyV1,
        capital: u64,
        span: u64,
        bond: PalwBondKeyV2,
        draw_daa: u64,
        n: u64,
        j: &mut Vec<PalwBudgetWriteV1>,
    ) -> Result<(), PalwBudgetRefusalV1> {
        if n == 0 {
            return Ok(());
        }
        let id = palw_round_rights_row_id_v1(span, &bond);
        let vector = palw_round_rights_vector_v1(policy, n);
        let caps = palw_bond_budget_caps_v1(policy, capital);
        let window = self.bonds.get(&bond).map(|r| r.window).unwrap_or_default();
        if let Some(dim) = window.checked_add(vector).ok_or(PalwBudgetRefusalV1::Overflow)?.first_excess(&caps) {
            return Err(PalwBudgetRefusalV1::Exhausted { dim });
        }
        self.reserve(policy, capital, id, bond, None, draw_daa, vector, PalwBudgetOriginV1::RoundRights, false, j)?;
        let mut row = *self.claims.get(&id).expect("reserved above");
        row.consumed = row.reserved;
        self.put_claim(&id, Some(row), j);
        self.close(&id, j);
        // Not a V2 claim: nothing else will ever forget it, so it leaves at its release.
        self.forget(&id, j);
        Ok(())
    }

    /// **Draw one physical reward block for `bond` from its open reservations** (hook H-3: a claim-backed / EXEC_SLICE reward block
    /// outside an escrowed claim), oldest release first, all-or-nothing.
    pub fn consume_block_for_bond(&mut self, bond: &PalwBondKeyV2, j: &mut Vec<PalwBudgetWriteV1>) -> Result<(), PalwBudgetRefusalV1> {
        let mut need = PALW_BUDGET_BLOCK_UNIT_V1 as u128;
        let mut plan: Vec<(Hash64, u128)> = Vec::new();
        for (_, claim_id) in self.releases.iter() {
            let Some(claim) =
                self.claims.get(claim_id).filter(|c| c.bond == *bond && c.open && c.origin != PalwBudgetOriginV1::Legacy)
            else {
                continue;
            };
            let left = (claim.reserved.block_units - claim.consumed.block_units.min(claim.reserved.block_units)) as u128;
            let take = left.min(need);
            if take > 0 {
                plan.push((*claim_id, take));
                need -= take;
            }
            if need == 0 {
                break;
            }
        }
        if need > 0 {
            return Err(PalwBudgetRefusalV1::Short { dim: PalwBudgetDimV1::BlockUnits });
        }
        for (claim_id, take) in plan {
            self.consume(&claim_id, PalwBudgetDimV1::BlockUnits, take, true, j);
        }
        Ok(())
    }

    // ---- ADR-0177 ----

    /// **Accept a bond's signed capital assignment** (design §3.1; the signature is the acceptance layer's): canonical entries, every
    /// model on this chain, `Σ ≤ capital`, a fresh sequence. It replaces the pending assignment and waits to season.
    #[allow(clippy::too_many_arguments)]
    pub fn assign(
        &mut self,
        policy: &PalwModelAllocationPolicyV1,
        bond: PalwBondKeyV2,
        assignments: &[(Hash64, u64)],
        sequence: u64,
        capital: u64,
        epoch: u64,
        model_exists: impl Fn(&Hash64) -> bool,
        j: &mut Vec<PalwBudgetWriteV1>,
    ) -> Result<(), PalwBudgetRefusalV1> {
        use PalwBudgetRefusalV1::Assignment as A;
        if assignments.len() > policy.max_models_per_bond as usize {
            return Err(A("more models than the policy's max_models_per_bond"));
        }
        if assignments.windows(2).any(|w| w[0].0 >= w[1].0) {
            return Err(A("model ids must be strictly ascending"));
        }
        if assignments.iter().any(|(_, amount)| *amount == 0) {
            return Err(A("an assigned amount must be positive"));
        }
        let total: u128 = assignments.iter().map(|(_, a)| *a as u128).sum();
        if total > capital as u128 {
            return Err(A("the assignments exceed the bond's locked capital"));
        }
        if assignments.iter().any(|(m, _)| !model_exists(m)) {
            return Err(A("a named model is not registered on this chain"));
        }
        let mut row = self.assignments.get(&bond).cloned().unwrap_or_default();
        if sequence <= row.sequence {
            return Err(A("the sequence is not above the bond's last accepted one"));
        }
        row.sequence = sequence;
        row.pending = Some(assignments.to_vec());
        row.pending_since_epoch = epoch;
        put(&mut self.assignments, PALW_BUDGET_TABLE_ASSIGNMENTS_V1, &bond, Some(row), j);
        Ok(())
    }

    /// **Add a chain block's realized PALW carve** to the current epoch (design §3.4). No-op without an allocation epoch.
    pub fn accrue(&mut self, carve: u64, j: &mut Vec<PalwBudgetWriteV1>) {
        let Some(mut epoch) = self.header.allocation else { return };
        epoch.accrued_sompi = epoch.accrued_sompi.saturating_add(carve);
        let mut header = self.header.clone();
        header.allocation = Some(epoch);
        self.put_header(header, j);
    }

    /// **Open allocation epoch `index` at `start_daa`** (design §3.2): promote seasoned assignments, take the snapshot (`capital_of` answers
    /// a bond's locked capital and whether it is Active), write every model's `(S_m, A_m)` fresh and drop the old rows, and reset the
    /// realized budget. Runs once per epoch, before admissions.
    pub fn roll_epoch(
        &mut self,
        policy: &PalwModelAllocationPolicyV1,
        index: u64,
        start_daa: u64,
        capital_of: impl Fn(&PalwBondKeyV2) -> (u64, bool),
        j: &mut Vec<PalwBudgetWriteV1>,
    ) {
        let bonds: Vec<PalwBondKeyV2> = self.assignments.keys().copied().collect();
        let mut s: BTreeMap<Hash64, u128> = BTreeMap::new();
        for bond in bonds {
            let mut row = self.assignments.get(&bond).cloned().expect("listed above");
            if row.pending.is_some()
                && index >= row.pending_since_epoch.saturating_add(1).saturating_add(policy.seasoning_epochs as u64)
            {
                row.effective = row.pending.take().expect("checked");
            }
            let (capital, active) = capital_of(&bond);
            for (model, amount) in palw_assignment_amounts_v1(&row, capital, active) {
                *s.entry(model).or_default() += amount as u128;
            }
            // A row that names nothing and waits for nothing has done its work; its sequence must survive, so it stays.
            put(&mut self.assignments, PALW_BUDGET_TABLE_ASSIGNMENTS_V1, &bond, Some(row), j);
        }
        let stale: Vec<Hash64> = self.models.keys().filter(|m| !s.contains_key(*m)).copied().collect();
        for model in stale {
            put(&mut self.models, PALW_BUDGET_TABLE_MODELS_V1, &model, None, j);
        }
        let mut sum_a = PalwAllocationWeightV1::ZERO;
        for (model, capital) in s {
            let weight = policy.curve.weight(capital);
            // Below 2^320 for any number of models a state can hold (each weight < 2^256): exact.
            sum_a = sum_a.saturating_add(weight);
            put(
                &mut self.models,
                PALW_BUDGET_TABLE_MODELS_V1,
                &model,
                Some(PalwModelBudgetRowV1 { epoch: index, capital, weight, reserved_sompi: 0 }),
                j,
            );
        }
        let mut header = self.header.clone();
        header.allocation = Some(PalwAllocationEpochV1 { index, start_daa, sum_a, accrued_sompi: 0 });
        self.put_header(header, j);
    }

    /// **Reserve reward from a model's budget** (design §3.4–3.5): grants `min(ask, available_m)` and records it. `0` with no
    /// allocation, an unknown or zero-weight model, or a zero denominator.
    pub fn reserve_model_reward(&mut self, model: &Hash64, ask: u64, j: &mut Vec<PalwBudgetWriteV1>) -> u64 {
        let grant = ask.min(self.model_available(model));
        if grant > 0 {
            let mut row = *self.models.get(model).expect("a positive availability has a row");
            row.reserved_sompi += grant;
            put(&mut self.models, PALW_BUDGET_TABLE_MODELS_V1, model, Some(row), j);
        }
        grant
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tx::TransactionOutpoint;

    fn bond(i: u32) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(Default::default(), i))
    }

    fn h(i: u64) -> Hash64 {
        Hash64::from_u64_word(i)
    }

    const BILI: u64 = crate::constants::SOMPI_PER_KASPA;

    /// A TEST policy (not a proposal): 1,000 BILI per unit, W = 100, q = 10, b = 4 blocks, r = 50 BILI, w = 400.
    fn policy(rho: u32, slice: bool) -> PalwBondBudgetPolicyV1 {
        // W = 100 is outside the validated 280..=436 on purpose: the engine is exercised below validation (as the fold fixtures are).
        PalwBondBudgetPolicyV1 {
            version: PALW_BOND_BUDGET_POLICY_VERSION_V2,
            window_daa: 100,
            capital_unit_sompi: 1_000 * BILI,
            rho,
            claims_per_unit: 10,
            block_units_per_unit: 4 * PALW_BUDGET_BLOCK_UNIT_V1,
            reward_per_unit_sompi: 50 * BILI,
            final_weight_per_unit: 400,
            max_open_claims_per_bond: 1_000_000,
            slice_rights_by_rho: slice,
            round_rights: PalwRoundRightsPolicyV1::CountAgainstBlocks,
            liability_hold_daa: 30,
            export_cap_permille: PALW_BOND_BUDGET_EXPORT_CAP_MAX_PERMILLE_V1,
            weight_value: PalwWeightValuePolicyV1::Unknown,
            hold: None,
        }
    }

    /// Deterministic xorshift for the property tests (no external crate).
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }
        fn below(&mut self, n: u64) -> u64 {
            self.next() % n.max(1)
        }
    }

    #[test]
    fn funded_reporter_award_above_one_window_pays_exact_tranches_across_restart_and_reorg() {
        for w in [280, 436] {
            let mut p = policy(1, false);
            p.window_daa = w;
            let capital = 1_000 * BILI;
            let recipient = bond(0);
            let total = 125 * BILI;
            let mut s = PalwBondBudgetStateV1::new(10, &p);
            let initial = s.clone();
            // The derived index adds no serialized field to dormant carriage.
            assert_eq!(
                enc(&s),
                enc(&(
                    s.header.clone(),
                    s.bonds.clone(),
                    s.claims.clone(),
                    s.releases.clone(),
                    s.assignments.clone(),
                    s.models.clone(),
                    s.liability_releases.clone()
                ))
            );
            let mut j = Vec::new();
            s.reserve_income_v1(&p, capital, h(100), recipient, 10, 50 * BILI, 50 * BILI, PalwBudgetOriginV1::MarketFee, &mut j)
                .unwrap();
            s.stage_reporter_income_v1(h(101), recipient, 20, total, &mut j).unwrap();
            let full = (s.clone(), j.len());
            assert_eq!(s.redeem_reporter_income_v1(&p, capital, &h(101), 20, h(102), u64::MAX, &mut j).unwrap(), 0);
            assert_eq!((s.clone(), j.len()), full, "R-full conviction retains exact debt without cash writes");
            s.forget(&h(101), &mut j);
            assert_eq!(s.claim_row(&h(101)).unwrap().reserved.reward_sompi, total);
            assert!(s.window_holds(&recipient, u64::MAX), "unpaid debt keeps the authenticated payee bond");
            s = borsh::from_slice(&enc(&s)).unwrap();
            assert!(s.window_holds(&recipient, u64::MAX), "restart reconstructs withdrawal liability");
            s.release_due(10 + w, &mut j);
            let first = s.redeem_reporter_income_v1(&p, capital, &h(101), 10 + w, h(102), u64::MAX, &mut j).unwrap();
            assert_eq!(first, 50 * BILI);
            assert_eq!(s.bond_row(&recipient).unwrap().window, PalwBudgetVectorV1 { reward_sompi: first, ..PalwBudgetVectorV1::ZERO });
            s.release_due(10 + 3 * w, &mut j);
            assert!(s.claim_row(&h(102)).unwrap().live, "an unpaid native queue cannot expire on logical W");
            let paid_first = 11 + 3 * w;
            s.confirm_native_income_payment_v1(&p, &h(102), paid_first, &mut j).unwrap();
            let before_second = s.clone();
            let before_second_j = j.len();
            s.release_due(paid_first + w, &mut j);
            let second = s.redeem_reporter_income_v1(&p, capital, &h(101), paid_first + w, h(103), 30 * BILI, &mut j).unwrap();
            assert_eq!(second, 30 * BILI, "the immutable award permits a native queue constrained tranche");
            s.confirm_native_income_payment_v1(&p, &h(103), paid_first + w + 7, &mut j).unwrap();
            let second_branch = s.clone();
            // Reorg only the second payment, retaining the real first payment.
            let mut reverted = second_branch.clone();
            for entry in j[before_second_j..].iter().rev() {
                let PalwBudgetWriteV1::Row { table, key, old, new } = entry else { panic!("only monetary rows") };
                reverted.apply_row(*table, key, old, new, true).unwrap();
            }
            assert_eq!(reverted, before_second);
            assert_eq!(reverted.claim_row(&h(101)).unwrap().consumed.reward_sompi, first);
            for entry in &j[before_second_j..] {
                let PalwBudgetWriteV1::Row { table, key, old, new } = entry else { panic!("only monetary rows") };
                reverted.apply_row(*table, key, old, new, false).unwrap();
            }
            assert_eq!(reverted, second_branch);
            s = borsh::from_slice(&enc(&reverted)).unwrap();
            let room = s.redeem_reporter_income_v1(&p, capital, &h(101), paid_first + w + 8, h(104), u64::MAX, &mut j).unwrap();
            assert_eq!(room, 20 * BILI);
            s.confirm_native_income_payment_v1(&p, &h(104), paid_first + w + 9, &mut j).unwrap();
            s.release_due(paid_first + 2 * w + 9, &mut j);
            let last = s.redeem_reporter_income_v1(&p, capital, &h(101), paid_first + 2 * w + 9, h(105), u64::MAX, &mut j).unwrap();
            assert_eq!(last, 25 * BILI);
            assert_eq!(first + second + room + last, total);
            assert_eq!(s.claim_row(&h(101)).unwrap().reserved.reward_sompi, total);
            assert_eq!(s.claim_row(&h(101)).unwrap().consumed.reward_sompi, total);
            assert!(s.window_holds(&recipient, paid_first + 4 * w), "last physical payment has not occurred");
            s.confirm_native_income_payment_v1(&p, &h(105), paid_first + 4 * w, &mut j).unwrap();
            s.release_due(paid_first + 5 * w, &mut j);
            assert!(!s.window_holds(&recipient, paid_first + 5 * w));
            let complete = (s.clone(), j.len());
            s.stage_reporter_income_v1(h(101), recipient, 20, total, &mut j).unwrap();
            assert_eq!(s.redeem_reporter_income_v1(&p, capital, &h(101), paid_first + 6 * w, h(106), u64::MAX, &mut j).unwrap(), 0);
            assert!(s.stage_reporter_income_v1(h(101), recipient, 20, total + 1, &mut j).is_err());
            assert_eq!((s.clone(), j.len()), complete, "no second classification, clipped award or excess payment");
            s.check_consistency().unwrap();
            let mut replay = initial;
            for entry in &j {
                let PalwBudgetWriteV1::Row { table, key, old, new } = entry else { panic!("only monetary rows") };
                replay.apply_row(*table, key, old, new, false).unwrap();
            }
            assert_eq!(replay, s);
            replay.check_consistency().unwrap();
        }
    }

    #[test]
    fn reporter_monetary_v2_freezes_payee_and_quarantines_legacy_cash_until_physical_pay_plus_w() {
        fn replay_write(s: &mut PalwBondBudgetStateV1, write: &PalwBudgetWriteV1, revert: bool) {
            match write {
                PalwBudgetWriteV1::Header { old, new } => {
                    let (expected, install) = if revert { (new, old) } else { (old, new) };
                    assert_eq!(Some(&s.header), expected.as_ref());
                    s.header = install.clone().unwrap();
                }
                PalwBudgetWriteV1::Row { table, key, old, new } => s.apply_row(*table, key, old, new, revert).unwrap(),
            }
        }
        for w in [280, 436] {
            let mut p = policy(1, false);
            p.window_daa = w;
            let capital = 1_000 * BILI;
            let mut s = PalwBondBudgetStateV1::new(10, &p);
            let initial = s.clone();
            let mut j = Vec::new();
            let binding = PalwReporterIncomeBindingV1 {
                recipient: Some(bond(1)),
                payload: h(91),
                maximum: 125 * BILI,
                accepted_daa: 20,
                awarded_daa: None,
                queue: None,
                paid_daa: None,
                historical_boundary: None,
            };
            s.bind_reporter_income_v1(h(500), binding, &mut j).unwrap();
            assert_eq!(s.header.version, PALW_BOND_BUDGET_STATE_VERSION_V2);
            assert!(s.window_holds(&bond(1), u64::MAX), "the named full entitlement keeps its actual recipient collectible");
            // Earlier valid reveal order is checked by the native consumer. Its
            // still-unawarded winner may improve, but an awarded right cannot.
            let winner = PalwReporterIncomeBindingV1 { recipient: Some(bond(2)), payload: h(92), ..binding };
            s.bind_reporter_income_v1(h(500), winner, &mut j).unwrap();
            assert!(!s.window_holds(&bond(1), u64::MAX));
            assert!(s.window_holds(&bond(2), u64::MAX));
            s.stage_reporter_income_v1(h(500), bond(2), 21, winner.maximum, &mut j).unwrap();
            s.mark_reporter_awarded_v1(h(500), 21, &mut j).unwrap();
            let awarded = (s.clone(), j.len());
            assert!(s.bind_reporter_income_v1(h(500), binding, &mut j).is_err());
            assert_eq!((s.clone(), j.len()), awarded, "awarded actual recipient, payout and total are immutable");
            let legacy = PalwReporterIncomeBindingV1 {
                recipient: None,
                payload: h(92),
                maximum: 9 * BILI,
                accepted_daa: 30,
                awarded_daa: None,
                queue: Some(h(501)),
                paid_daa: None,
                historical_boundary: None,
            };
            s.bind_reporter_income_v1(h(502), legacy, &mut j).unwrap();
            let history = PalwReporterIncomeBindingV1 {
                recipient: None,
                payload: h(0),
                maximum: 0,
                accepted_daa: 30,
                awarded_daa: None,
                queue: None,
                paid_daa: None,
                historical_boundary: Some((9 * u128::from(BILI), 30 + w)),
            };
            s.bind_reporter_income_v1(h(503), history, &mut j).unwrap();
            assert!(s.has_unattributed_income_v1());
            let held = (s.clone(), j.len());
            let refusal = PalwBudgetRefusalV1::Exhausted { dim: PalwBudgetDimV1::Reward };
            assert_eq!(
                s.reserve_income_v1(&p, capital, h(510), bond(3), 31, 1, 1, PalwBudgetOriginV1::MarketFee, &mut j),
                Err(refusal)
            );
            assert_eq!(s.reserve_source_income_v1(&p, capital, h(511), bond(3), 31, 100, 1, &mut j), Err(refusal));
            assert_eq!(s.hold_native_income_v1(&p, capital, h(512), bond(3), 31, 100, 1, &[], true, &mut j), Err(refusal));
            assert_eq!(
                s.preview_claim_v1(&p, capital, bond(3), None, PalwBudgetAskV1 { reward_sompi: 1, ..Default::default() }),
                Err(refusal)
            );
            assert_eq!(s.redeem_reporter_income_v1(&p, capital, &h(500), 31, h(513), u64::MAX, &mut j).unwrap(), 0);
            assert_eq!((s.clone(), j.len()), held, "quarantine denies only new R, never consumes debt or any budget dimension");
            s = borsh::from_slice(&enc(&s)).unwrap();
            assert!(s.has_unattributed_income_v1());
            assert!(s.window_holds(&bond(2), u64::MAX));
            s.release_reporter_income_v1(&p, 30 + w, &mut j);
            assert!(!s.reporter_income.contains_key(&h(503)), "the history uncertainty boundary is first fence + W");
            assert!(s.has_unattributed_income_v1(), "a still-unpaid exact queue cannot expire with the history boundary");
            s.release_reporter_income_v1(&p, 30 + 10 * w, &mut j);
            assert!(s.has_unattributed_income_v1());
            let paid = 31 + 10 * w;
            s.confirm_unattributed_payment_v1(h(502), paid, &mut j).unwrap();
            let paid_state = s.clone();
            s.release_reporter_income_v1(&p, paid + w - 1, &mut j);
            assert!(s.has_unattributed_income_v1());
            s.release_reporter_income_v1(&p, paid + w, &mut j);
            assert!(!s.has_unattributed_income_v1());
            assert_eq!(s.redeem_reporter_income_v1(&p, capital, &h(500), paid + w, h(514), u64::MAX, &mut j).unwrap(), 50 * BILI);
            assert_eq!(
                s.reporter_income[&h(500)].payload,
                h(92),
                "the debt redeems the frozen winner, never a matching address's owner"
            );
            s.check_consistency().unwrap();
            assert_eq!(borsh::from_slice::<PalwBondBudgetStateV1>(&enc(&paid_state)).unwrap(), paid_state);
            let mut different = s.clone();
            different.reporter_income.get_mut(&h(500)).unwrap().payload = h(93);
            assert_ne!(
                different.root_parts(),
                s.root_parts(),
                "table6 frozen payload is rooted even though v1's five-table API is unchanged"
            );
            let mut replay = initial.clone();
            for write in &j {
                replay_write(&mut replay, write, false);
            }
            assert_eq!(replay, s);
            for write in j.iter().rev() {
                replay_write(&mut replay, write, true);
            }
            assert_eq!(replay, initial, "reorg restores the original v1 schema and exact wire bytes");
            replay.check_consistency().unwrap();
        }
    }

    #[test]
    fn source_fee_reservation_survives_h_and_sparse_ticks_then_counts_w_after_actual_payment() {
        let p = policy(1, false);
        let capital = 1_000 * BILI;
        let fee = 50 * BILI;
        let mut s = PalwBondBudgetStateV1::new(10, &p);
        let empty = s.clone();
        let mut j = Vec::new();
        s.reserve_source_income_v1(&p, capital, h(50), bond(0), 10, 500, fee, &mut j).unwrap();
        let row = s.claim_row(&h(50)).unwrap();
        assert!(!row.open && row.live && row.in_window);
        assert_eq!(row.reuse_not_before, 600);
        assert_eq!(s.bond_row(&bond(0)).unwrap().open_claims, 0);
        s.release_due(110, &mut j);
        s.release_due(600, &mut j);
        s.release_due(900, &mut j);
        assert_eq!(s.bond_row(&bond(0)).unwrap().window, PalwBudgetVectorV1 { reward_sompi: fee, ..PalwBudgetVectorV1::ZERO });
        let before = (s.root_parts(), j.len());
        assert!(s.reserve_income_v1(&p, capital, h(51), bond(0), 900, 1, 1, PalwBudgetOriginV1::MarketFee, &mut j).is_err());
        assert!(s.settle_source_income_v1(&p, &h(50), 500, 500, fee, &mut j).is_err(), "H is inclusive");
        assert!(s.settle_source_income_v1(&p, &h(50), 500, 900, fee - 1, &mut j).is_err());
        assert_eq!((s.root_parts(), j.len()), before, "every refusal precedes its journal writes");
        s.settle_source_income_v1(&p, &h(50), 500, 900, fee, &mut j).unwrap();
        assert_eq!(s.claim_row(&h(50)).unwrap().consumed.reward_sompi, fee);
        assert_eq!(s.claim_row(&h(50)).unwrap().reuse_not_before, 1_000);
        let paid = (s.root_parts(), j.len());
        assert!(s.settle_source_income_v1(&p, &h(50), 500, 901, fee, &mut j).is_err());
        assert!(s.cancel_source_income_v1(&h(50), &mut j).is_err());
        assert_eq!((s.root_parts(), j.len()), paid, "a paid right cannot pay or refund again");
        s.release_due(999, &mut j);
        assert_eq!(s.bond_row(&bond(0)).unwrap().window.reward_sompi, fee);
        s.release_due(1_000, &mut j);
        assert!(s.bond_row(&bond(0)).is_none() && s.claim_row(&h(50)).is_none());
        s.check_consistency().unwrap();
        let mut replay = empty.clone();
        for entry in &j {
            match entry {
                PalwBudgetWriteV1::Row { table, key, old, new } => replay.apply_row(*table, key, old, new, false).unwrap(),
                PalwBudgetWriteV1::Header { .. } => panic!("income does not alter the policy header"),
            }
        }
        assert_eq!(replay.root_parts(), s.root_parts());
        for entry in j.iter().rev() {
            if let PalwBudgetWriteV1::Row { table, key, old, new } = entry {
                replay.apply_row(*table, key, old, new, true).unwrap();
            }
        }
        assert_eq!(replay.root_parts(), empty.root_parts(), "undo restores the original empty native ledger");
    }

    #[test]
    fn native_income_reclassifies_kernel_r_without_q_b_f_t_or_double_charge_and_pays_after_sparse_jump() {
        let p = policy(1, false);
        let capital = 1_000 * BILI;
        let mut s = PalwBondBudgetStateV1::new(10, &p);
        let empty = s.clone();
        let mut j = Vec::new();
        let kernel = PalwBudgetVectorV1 { claims: 1, reward_sompi: 40 * BILI, ..PalwBudgetVectorV1::ZERO };
        s.reserve(&p, capital, h(70), bond(0), None, 10, kernel, PalwBudgetOriginV1::KernelRoute, true, &mut j).unwrap();
        s.hold_native_income_v1(&p, capital, h(71), bond(0), 20, 500, 45 * BILI, &[h(70)], true, &mut j).unwrap();
        assert_eq!(s.claim_row(&h(70)).unwrap().reserved.reward_sompi, 0);
        assert_eq!(s.bond_row(&bond(0)).unwrap().window, PalwBudgetVectorV1 { reward_sompi: 45 * BILI, ..kernel });
        assert_eq!(s.bond_row(&bond(0)).unwrap().open_claims, 1);
        s.close(&h(70), &mut j);
        s.forget(&h(70), &mut j);
        s.release_due(900, &mut j);
        assert_eq!(s.bond_row(&bond(0)).unwrap().window, PalwBudgetVectorV1 { reward_sompi: 45 * BILI, ..PalwBudgetVectorV1::ZERO });
        assert!(s.window_holds(&bond(0), 900), "unpaid income still holds collateral past its initial H+W");
        let before = (s.root_parts(), j.len());
        assert!(s.hold_native_income_v1(&p, capital, h(71), bond(0), 900, 1_000, 51 * BILI, &[], true, &mut j).is_err());
        assert_eq!((s.root_parts(), j.len()), before, "refusal reclassifies and reserves nothing");
        s.pay_native_income_v1(&p, &h(71), 901, 5 * BILI, h(72), false, &mut j).unwrap();
        s.pay_native_income_v1(&p, &h(71), 902, 35 * BILI, h(72), true, &mut j).unwrap();
        assert!(s.claim_row(&h(71)).is_none());
        assert_eq!(s.claim_row(&h(72)).unwrap().origin, PalwBudgetOriginV1::NativeIncomeQueued);
        s.release_due(2_000, &mut j);
        assert!(s.window_holds(&bond(0), 2_000), "queued income also keeps the physical withdrawal gate closed");
        assert_eq!(s.bond_row(&bond(0)).unwrap().window.reward_sompi, 45 * BILI, "queued money cannot expire before actual payment");
        s.confirm_native_income_payment_v1(&p, &h(72), 2_000, &mut j).unwrap();
        assert_eq!(s.claim_row(&h(72)).unwrap().reuse_not_before, 2_100);
        assert!(s.pay_native_income_v1(&p, &h(71), 2_000, 1, h(72), false, &mut j).is_err());
        assert!(s.confirm_native_income_payment_v1(&p, &h(72), 2_001, &mut j).is_err());
        s.check_consistency().unwrap();
        s.release_due(2_100, &mut j);
        assert!(s.bond_row(&bond(0)).is_none());
        let mut replay = empty.clone();
        for entry in &j {
            if let PalwBudgetWriteV1::Row { table, key, old, new } = entry {
                replay.apply_row(*table, key, old, new, false).unwrap();
            }
        }
        assert_eq!(replay.root_parts(), s.root_parts());
        for entry in j.iter().rev() {
            if let PalwBudgetWriteV1::Row { table, key, old, new } = entry {
                replay.apply_row(*table, key, old, new, true).unwrap();
            }
        }
        assert_eq!(replay.root_parts(), empty.root_parts());
    }

    #[test]
    fn round_candidates_and_reservations_agree_when_legacy_money_exceeds_the_new_cap() {
        let mut p = policy(1, false);
        p.round_rights = PalwRoundRightsPolicyV1::ExecutionCap { rights_per_unit: 1 };
        let capital = 1_000 * BILI;
        let caps = palw_bond_budget_caps_v1(&p, capital);
        for dim in [
            PalwBudgetDimV1::Claims,
            PalwBudgetDimV1::BlockUnits,
            PalwBudgetDimV1::Reward,
            PalwBudgetDimV1::FinalWeight,
            PalwBudgetDimV1::RoundRights,
        ] {
            let mut s = PalwBondBudgetStateV1::new(10, &p);
            let mut j = Vec::new();
            let mut old = PalwBudgetVectorV1::ZERO;
            match dim {
                PalwBudgetDimV1::Claims => old.claims = caps.claims + 1,
                PalwBudgetDimV1::BlockUnits => old.block_units = caps.block_units + 1,
                PalwBudgetDimV1::Reward => old.reward_sompi = caps.reward_sompi + 1,
                PalwBudgetDimV1::FinalWeight => old.final_weight = caps.final_weight + 1,
                PalwBudgetDimV1::RoundRights => old.round_rights = caps.round_rights + 1,
            }
            s.reserve(&p, capital, h(64), bond(0), None, 10, old, PalwBudgetOriginV1::Legacy, false, &mut j).unwrap();
            assert_eq!(palw_round_rights_remaining_v1(&p, capital, s.bond_row(&bond(0))), 0, "overfull {dim:?}");
            let before = (s.root_parts(), j.len());
            assert!(matches!(
                s.reserve_round_rights_v1(&p, capital, 1, bond(0), 11, 1, &mut j),
                Err(PalwBudgetRefusalV1::Exhausted { .. })
            ));
            assert_eq!((s.root_parts(), j.len()), before);
            s.reserve_round_rights_v1(&p, capital, 1, bond(0), 11, 0, &mut j).unwrap();
            assert_eq!((s.root_parts(), j.len()), before, "the zero candidate draw preserves accepted old rights");
        }
    }

    #[test]
    fn partial_native_funds_keep_each_fd_fe_queue_until_its_own_payment_clock_in_both_orders() {
        // The core engine receives exact native queue identities. These prefix
        // variants model FD kernel fees and FE native funds in either sort order;
        // actual pre-object draining is covered separately by the fold fixture.
        for (first, last) in [(h(0xfe), h(0xfd)), (h(0xfd), h(0xfe))] {
            let p = policy(1, false);
            let capital = 1_000 * BILI;
            let mut s = PalwBondBudgetStateV1::new(10, &p);
            let initial = s.clone();
            let mut j = Vec::new();
            s.hold_native_income_v1(&p, capital, h(68), bond(0), 10, 500, 50 * BILI, &[], true, &mut j).unwrap();
            s.pay_native_income_v1(&p, &h(68), 501, 10 * BILI, first, false, &mut j).unwrap();
            let remainder = s.claim_row(&h(68)).unwrap();
            assert_eq!(
                (remainder.origin, remainder.reserved.reward_sompi, remainder.consumed.reward_sompi),
                (PalwBudgetOriginV1::NativeIncomePartial, 40 * BILI, 0)
            );
            assert_eq!(s.claim_row(&first).unwrap().reserved.reward_sompi, 10 * BILI);
            let before = (s.root_parts(), j.len());
            assert!(s.hold_native_income_v1(&p, capital, h(69), bond(0), 502, 600, 40 * BILI, &[h(68)], true, &mut j).is_err());
            assert_eq!((s.root_parts(), j.len()), before, "a partly paid right cannot become another EXEC entitlement");
            s.pay_native_income_v1(&p, &h(68), 503, 20 * BILI, last, true, &mut j).unwrap();
            assert!(s.claim_row(&h(68)).is_none());
            assert_eq!(
                (s.claim_row(&last).unwrap().reserved.reward_sompi, s.claim_row(&last).unwrap().consumed.reward_sompi),
                (40 * BILI, 20 * BILI)
            );
            assert_eq!(s.bond_row(&bond(0)).unwrap().window.reward_sompi, 50 * BILI);
            // The last-created queue drains first. Its W must not free the
            // earlier queue that is still actual unpaid money.
            s.confirm_native_income_payment_v1(&p, &last, 1_000, &mut j).unwrap();
            s.release_due(1_100, &mut j);
            assert!(s.claim_row(&last).is_none());
            assert!(s.claim_row(&first).unwrap().live);
            assert_eq!(s.bond_row(&bond(0)).unwrap().window.reward_sompi, 10 * BILI);
            s.release_due(2_000, &mut j);
            assert!(s.window_holds(&bond(0), 2_000));
            s.confirm_native_income_payment_v1(&p, &first, 3_000, &mut j).unwrap();
            s.release_due(3_099, &mut j);
            assert_eq!(s.bond_row(&bond(0)).unwrap().window.reward_sompi, 10 * BILI);
            s.release_due(3_100, &mut j);
            assert!(s.bond_row(&bond(0)).is_none());
            s.check_consistency().unwrap();
            let mut replay = initial.clone();
            for entry in &j {
                if let PalwBudgetWriteV1::Row { table, key, old, new } = entry {
                    replay.apply_row(*table, key, old, new, false).unwrap();
                }
            }
            assert_eq!(replay.root_parts(), s.root_parts());
            for entry in j.iter().rev() {
                if let PalwBudgetWriteV1::Row { table, key, old, new } = entry {
                    replay.apply_row(*table, key, old, new, true).unwrap();
                }
            }
            assert_eq!(replay.root_parts(), initial.root_parts());
        }
    }

    #[test]
    fn partial_native_cash_survives_remainder_cancellation_and_branch_replay_without_second_payment() {
        let p = policy(1, false);
        let capital = 1_000 * BILI;
        let mut s = PalwBondBudgetStateV1::new(10, &p);
        let mut j = Vec::new();
        s.hold_native_income_v1(&p, capital, h(65), bond(0), 10, 500, 50 * BILI, &[], true, &mut j).unwrap();
        s.pay_native_income_v1(&p, &h(65), 501, 10 * BILI, h(66), false, &mut j).unwrap();
        s.confirm_native_income_payment_v1(&p, &h(66), 600, &mut j).unwrap();
        let paid_branch = s.clone();
        let start = j.len();
        // This is the monetary cancellation primitive used by post-Final
        // conviction. It may cancel only the unpaid 40, not the 10 actual cash.
        s.cancel_native_income_v1(&h(65), &mut j).unwrap();
        assert!(!s.claim_row(&h(65)).unwrap().live);
        assert!(!s.claim_row(&h(66)).unwrap().live);
        assert_eq!(s.bond_row(&bond(0)).unwrap().window.reward_sompi, 50 * BILI);
        let cancelled = s.root_parts();
        let before = (cancelled, j.len());
        assert!(s.pay_native_income_v1(&p, &h(65), 601, 40 * BILI, h(67), true, &mut j).is_err());
        assert!(s.hold_native_income_v1(&p, capital, h(65), bond(0), 601, 900, 40 * BILI, &[], true, &mut j).is_err());
        assert!(s.confirm_native_income_payment_v1(&p, &h(66), 601, &mut j).is_err());
        assert_eq!((s.root_parts(), j.len()), before, "neither unpaid revival nor duplicate paid cash is admitted");
        // A reorg that keeps the actual payment but removes the later
        // conviction restores only its legitimate remaining entitlement.
        for entry in j[start..].iter().rev() {
            if let PalwBudgetWriteV1::Row { table, key, old, new } = entry {
                s.apply_row(*table, key, old, new, true).unwrap();
            }
        }
        assert_eq!(s.root_parts(), paid_branch.root_parts());
        assert!(!s.claim_row(&h(66)).unwrap().live, "the retained physical payment never becomes unpaid again");
        for entry in &j[start..] {
            if let PalwBudgetWriteV1::Row { table, key, old, new } = entry {
                s.apply_row(*table, key, old, new, false).unwrap();
            }
        }
        assert_eq!(s.root_parts(), cancelled);
        s.release_due(700, &mut j);
        assert!(s.bond_row(&bond(0)).is_none());
        s.check_consistency().unwrap();
    }

    #[test]
    fn standalone_income_transfers_to_exec_once_without_a_second_payment_or_q_charge() {
        let p = policy(1, false);
        let capital = 1_000 * BILI;
        let mut s = PalwBondBudgetStateV1::new(10, &p);
        let empty = s.clone();
        let mut j = Vec::new();
        s.reserve(
            &p,
            capital,
            h(78),
            bond(0),
            None,
            10,
            PalwBudgetVectorV1 { claims: 1, reward_sompi: 20 * BILI, ..PalwBudgetVectorV1::ZERO },
            PalwBudgetOriginV1::KernelRoute,
            true,
            &mut j,
        )
        .unwrap();
        s.hold_native_income_v1(&p, capital, h(79), bond(0), 10, 500, 20 * BILI, &[h(78)], false, &mut j).unwrap();
        s.release_due(700, &mut j);
        assert_eq!(s.bond_row(&bond(0)).unwrap().window.reward_sompi, 20 * BILI);
        s.hold_native_income_v1(&p, capital, h(80), bond(0), 701, 1_000, 25 * BILI, &[h(79)], true, &mut j).unwrap();
        assert!(s.claim_row(&h(79)).is_none(), "no remaining standalone payment identity");
        assert_eq!(s.bond_row(&bond(0)).unwrap().window.reward_sompi, 25 * BILI);
        assert_eq!(s.bond_row(&bond(0)).unwrap().open_claims, 1, "only the actual claim owns Q");
        assert!(s.pay_native_income_v1(&p, &h(79), 1_001, 20 * BILI, h(81), true, &mut j).is_err());
        s.pay_native_income_v1(&p, &h(80), 1_001, 25 * BILI, h(81), true, &mut j).unwrap();
        s.confirm_native_income_payment_v1(&p, &h(81), 1_002, &mut j).unwrap();
        s.close(&h(78), &mut j);
        s.forget(&h(78), &mut j);
        s.check_consistency().unwrap();
        let mut replay = empty.clone();
        for entry in &j {
            if let PalwBudgetWriteV1::Row { table, key, old, new } = entry {
                replay.apply_row(*table, key, old, new, false).unwrap();
            }
        }
        assert_eq!(replay.root_parts(), s.root_parts());
        for entry in j.iter().rev() {
            if let PalwBudgetWriteV1::Row { table, key, old, new } = entry {
                replay.apply_row(*table, key, old, new, true).unwrap();
            }
        }
        assert_eq!(replay.root_parts(), empty.root_parts());
    }

    #[test]
    fn native_income_coalesces_the_same_queue_recipient_across_actual_payments_without_freeing_r() {
        let p = policy(1, false);
        let capital = 1_000 * BILI;
        let mut s = PalwBondBudgetStateV1::new(10, &p);
        let mut j = Vec::new();
        s.hold_native_income_v1(&p, capital, h(75), bond(0), 10, 20, 10 * BILI, &[], true, &mut j).unwrap();
        s.pay_native_income_v1(&p, &h(75), 21, 8 * BILI, h(76), true, &mut j).unwrap();
        s.confirm_native_income_payment_v1(&p, &h(76), 22, &mut j).unwrap();
        s.hold_native_income_v1(&p, capital, h(77), bond(0), 23, 30, 20 * BILI, &[], true, &mut j).unwrap();
        s.pay_native_income_v1(&p, &h(77), 31, 17 * BILI, h(76), true, &mut j).unwrap();
        let coalesced = s.claim_row(&h(76)).unwrap();
        assert_eq!((coalesced.reserved.reward_sompi, coalesced.consumed.reward_sompi), (30 * BILI, 25 * BILI));
        assert!(coalesced.live);
        s.release_due(1_000, &mut j);
        assert_eq!(s.bond_row(&bond(0)).unwrap().window.reward_sompi, 30 * BILI);
        s.confirm_native_income_payment_v1(&p, &h(76), 1_000, &mut j).unwrap();
        s.release_due(1_099, &mut j);
        assert_eq!(s.bond_row(&bond(0)).unwrap().window.reward_sompi, 30 * BILI);
        s.release_due(1_100, &mut j);
        assert!(s.bond_row(&bond(0)).is_none());
        s.check_consistency().unwrap();
    }

    #[test]
    fn native_income_refuses_foreign_or_paid_credit_atomically_and_cancellation_does_not_free_r() {
        let p = policy(1, false);
        let capital = 1_000 * BILI;
        let mut s = PalwBondBudgetStateV1::new(10, &p);
        let mut j = Vec::new();
        let kernel = PalwBudgetVectorV1 { reward_sompi: 20 * BILI, ..PalwBudgetVectorV1::ZERO };
        s.reserve(&p, capital, h(73), bond(1), None, 10, kernel, PalwBudgetOriginV1::KernelRoute, true, &mut j).unwrap();
        let before = (s.root_parts(), j.len());
        assert!(s.hold_native_income_v1(&p, capital, h(74), bond(0), 20, 500, 20 * BILI, &[h(73)], true, &mut j).is_err());
        assert_eq!((s.root_parts(), j.len()), before);
        s.hold_native_income_v1(&p, capital, h(74), bond(0), 20, 500, 20 * BILI, &[], true, &mut j).unwrap();
        s.cancel_native_income_v1(&h(74), &mut j).unwrap();
        assert_eq!(s.bond_row(&bond(0)).unwrap().window.reward_sompi, 20 * BILI);
        // A redraw for the same one-pay root may resume its unspent maximum;
        // no additional quota or second payment entitlement is created.
        s.hold_native_income_v1(&p, capital, h(74), bond(0), 30, 500, 20 * BILI, &[], true, &mut j).unwrap();
        assert_eq!(s.bond_row(&bond(0)).unwrap().window.reward_sompi, 20 * BILI);
        s.pay_native_income_v1(&p, &h(74), 501, 1, h(72), false, &mut j).unwrap();
        let before = (s.root_parts(), j.len());
        assert!(s.hold_native_income_v1(&p, capital, h(74), bond(0), 502, 700, 20 * BILI, &[], true, &mut j).is_err());
        assert_eq!((s.root_parts(), j.len()), before);
        s.check_consistency().unwrap();
    }

    #[test]
    fn source_queue_credit_holds_r_and_capital_until_actual_coinbase_plus_w() {
        let p = policy(1, false);
        let capital = 1_000 * BILI;
        let mut s = PalwBondBudgetStateV1::new(10, &p);
        let empty = s.clone();
        let mut j = Vec::new();
        s.reserve_source_income_v1(&p, capital, h(82), bond(0), 10, 500, 20 * BILI, &mut j).unwrap();
        let before = (s.root_parts(), j.len());
        assert!(s.queue_source_income_v1(&p, &h(82), 500, 500, 20 * BILI, h(83), &mut j).is_err());
        assert!(s.queue_source_income_v1(&p, &h(82), 500, 501, 19 * BILI, h(83), &mut j).is_err());
        assert!(s.queue_source_income_v1(&p, &h(82), 500, 501, 20 * BILI, h(82), &mut j).is_err());
        assert_eq!((s.root_parts(), j.len()), before, "H, fixed fee and destination identity fail atomically");
        s.release_due(900, &mut j);
        assert!(s.window_holds(&bond(0), 900));
        s.queue_source_income_v1(&p, &h(82), 500, 901, 20 * BILI, h(83), &mut j).unwrap();
        assert!(s.claim_row(&h(82)).is_none(), "the source cannot obtain a second queue credit");
        let queued = s.claim_row(&h(83)).unwrap();
        assert_eq!(queued.origin, PalwBudgetOriginV1::NativeIncomeQueued);
        assert_eq!((queued.reserved.reward_sompi, queued.consumed.reward_sompi), (20 * BILI, 20 * BILI));
        assert!(queued.live && !queued.open);
        assert_eq!(s.bond_row(&bond(0)).unwrap().window, PalwBudgetVectorV1 { reward_sompi: 20 * BILI, ..PalwBudgetVectorV1::ZERO });
        assert!(s.queue_source_income_v1(&p, &h(82), 500, 902, 20 * BILI, h(83), &mut j).is_err());
        s.release_due(2_000, &mut j);
        assert!(s.window_holds(&bond(0), 2_000), "an unspent queue credit cannot free collateral");
        s.confirm_native_income_payment_v1(&p, &h(83), 2_000, &mut j).unwrap();
        assert_eq!(s.claim_row(&h(83)).unwrap().reuse_not_before, 2_100);
        s.release_due(2_099, &mut j);
        assert_eq!(s.bond_row(&bond(0)).unwrap().window.reward_sompi, 20 * BILI);
        s.release_due(2_100, &mut j);
        assert!(s.bond_row(&bond(0)).is_none());
        s.check_consistency().unwrap();
        let mut replay = empty.clone();
        for entry in &j {
            if let PalwBudgetWriteV1::Row { table, key, old, new } = entry {
                replay.apply_row(*table, key, old, new, false).unwrap();
            }
        }
        assert_eq!(replay.root_parts(), s.root_parts());
        for entry in j.iter().rev() {
            if let PalwBudgetWriteV1::Row { table, key, old, new } = entry {
                replay.apply_row(*table, key, old, new, true).unwrap();
            }
        }
        assert_eq!(replay.root_parts(), empty.root_parts());
    }

    #[test]
    fn source_queue_destination_collision_refuses_without_consuming_the_signed_promise() {
        let p = policy(1, false);
        let capital = 1_000 * BILI;
        let mut s = PalwBondBudgetStateV1::new(10, &p);
        let mut j = Vec::new();
        s.reserve_source_income_v1(&p, capital, h(84), bond(0), 10, 500, 20 * BILI, &mut j).unwrap();
        s.hold_native_income_v1(&p, capital, h(85), bond(1), 10, 500, 10 * BILI, &[], true, &mut j).unwrap();
        s.pay_native_income_v1(&p, &h(85), 501, 10 * BILI, h(86), true, &mut j).unwrap();
        let before = (s.root_parts(), j.len());
        assert!(s.queue_source_income_v1(&p, &h(84), 500, 501, 20 * BILI, h(86), &mut j).is_err());
        assert_eq!((s.root_parts(), j.len()), before);
        assert_eq!(s.claim_row(&h(84)).unwrap().consumed.reward_sompi, 0);
        assert!(s.claim_row(&h(84)).unwrap().live);
    }

    #[test]
    fn old_source_income_migrates_overfull_after_old_w_and_h_then_replays_and_pays_once() {
        let p = policy(1, false);
        let capital = 1_000 * BILI;
        let fee = 51 * BILI; // Today's R cap is 50; the pre-fence signed right remains collectible.
        let mut s = PalwBondBudgetStateV1::new(900, &p);
        let empty = s.clone();
        let mut j = Vec::new();
        s.seed_source_income_v1(&p, capital, h(52), bond(0), 10, 500, fee, &mut j).unwrap();
        let row = *s.claim_row(&h(52)).unwrap();
        assert_eq!(row.accepted_daa, 10);
        assert_eq!(row.reuse_not_before, 600);
        assert!(!row.open && row.live && row.in_window);
        s.release_due(900, &mut j);
        assert!(s.window_holds(&bond(0), 900), "old unpaid fee still holds collateral beyond signed H+W");
        let held = *s.bond_row(&bond(0)).unwrap();
        assert_eq!(held.window, PalwBudgetVectorV1 { reward_sompi: fee, ..PalwBudgetVectorV1::ZERO });
        assert_eq!(held.open_claims, 0);
        let before = (s.root_parts(), j.len());
        assert!(s.reserve_source_income_v1(&p, capital, h(53), bond(0), 900, 1_500, 1, &mut j).is_err());
        assert!(s.reserve_income_v1(&p, capital, h(54), bond(0), 900, 1, 1, PalwBudgetOriginV1::MarketFee, &mut j).is_err());
        assert!(s.seed_source_income_v1(&p, capital, h(52), bond(0), 10, 500, fee, &mut j).is_err());
        assert_eq!((s.root_parts(), j.len()), before);
        s.settle_source_income_v1(&p, &h(52), 500, 901, fee, &mut j).unwrap();
        s.release_due(1_000, &mut j);
        assert_eq!(s.bond_row(&bond(0)).unwrap().window.reward_sompi, fee);
        s.release_due(1_001, &mut j);
        assert!(s.claim_row(&h(52)).is_none() && s.bond_row(&bond(0)).is_none());
        s.check_consistency().unwrap();
        let mut replay = empty.clone();
        for entry in &j {
            if let PalwBudgetWriteV1::Row { table, key, old, new } = entry {
                replay.apply_row(*table, key, old, new, false).unwrap();
            }
        }
        assert_eq!(replay.root_parts(), s.root_parts());
        for entry in j.iter().rev() {
            if let PalwBudgetWriteV1::Row { table, key, old, new } = entry {
                replay.apply_row(*table, key, old, new, true).unwrap();
            }
        }
        assert_eq!(replay.root_parts(), empty.root_parts());
    }

    #[test]
    fn old_paid_source_income_uses_actual_payment_window_at_migration() {
        let p = policy(1, false);
        let fee = 51 * BILI;
        let mut s = PalwBondBudgetStateV1::new(950, &p);
        let mut j = Vec::new();
        s.seed_source_income_v1(&p, 1_000 * BILI, h(55), bond(0), 10, 500, fee, &mut j).unwrap();
        s.settle_source_income_v1(&p, &h(55), 500, 900, fee, &mut j).unwrap();
        let row = s.claim_row(&h(55)).unwrap();
        assert_eq!(row.reuse_not_before, 1_000);
        assert_eq!(row.consumed.reward_sompi, fee);
        assert!(!row.live && !row.open && row.in_window);
        s.release_due(999, &mut j);
        assert_eq!(s.bond_row(&bond(0)).unwrap().window.reward_sompi, fee);
        s.release_due(1_000, &mut j);
        assert!(s.bond_row(&bond(0)).is_none());
        s.check_consistency().unwrap();
    }

    #[test]
    fn source_fee_refund_cancels_the_entitlement_without_reusing_its_reserved_window() {
        let p = policy(1, false);
        let mut s = PalwBondBudgetStateV1::new(10, &p);
        let mut j = Vec::new();
        let empty = (s.root_parts(), j.len());
        assert!(s.reserve_source_income_v1(&p, 1_000 * BILI, h(60), bond(0), 10, u64::MAX, 1, &mut j).is_err());
        assert!(s.reserve_source_income_v1(&p, 1_000 * BILI, h(60), bond(0), 10, 9, 1, &mut j).is_err());
        assert_eq!((s.root_parts(), j.len()), empty);
        s.reserve_source_income_v1(&p, 1_000 * BILI, h(60), bond(0), 10, 500, 50 * BILI, &mut j).unwrap();
        s.cancel_source_income_v1(&h(60), &mut j).unwrap();
        let canceled = (s.root_parts(), j.len());
        assert!(s.settle_source_income_v1(&p, &h(60), 500, 501, 50 * BILI, &mut j).is_err());
        assert!(s.cancel_source_income_v1(&h(60), &mut j).is_err());
        assert_eq!((s.root_parts(), j.len()), canceled);
        s.release_due(599, &mut j);
        assert_eq!(s.bond_row(&bond(0)).unwrap().window.reward_sompi, 50 * BILI);
        s.release_due(600, &mut j);
        assert!(s.bond_row(&bond(0)).is_none() && s.claim_row(&h(60)).is_none());
        s.check_consistency().unwrap();
    }

    #[test]
    fn collected_market_fee_waits_for_physical_queue_payment_without_second_r_or_claims() {
        let p = policy(1, false);
        let mut s = PalwBondBudgetStateV1::new(10, &p);
        let mut j = Vec::new();
        let empty = s.clone();
        s.reserve_income_v1(&p, 1_000 * BILI, h(94), bond(0), 10, 50 * BILI, 45 * BILI, PalwBudgetOriginV1::MarketFee, &mut j)
            .unwrap();
        let before = (s.root_parts(), j.len());
        assert!(s.queue_collected_income_v1(&h(94), h(94), &mut j).is_err());
        assert_eq!((s.root_parts(), j.len()), before);
        s.queue_collected_income_v1(&h(94), h(95), &mut j).unwrap();
        assert!(s.claim_row(&h(94)).is_none());
        assert_eq!(s.claim_row(&h(95)).unwrap().origin, PalwBudgetOriginV1::NativeIncomeQueued);
        s.release_due(1_000, &mut j);
        assert_eq!(s.bond_row(&bond(0)).unwrap().window, PalwBudgetVectorV1 { reward_sompi: 50 * BILI, ..PalwBudgetVectorV1::ZERO });
        assert!(s.window_holds(&bond(0), 1_000));
        let before = (s.root_parts(), j.len());
        assert!(s.queue_collected_income_v1(&h(94), h(95), &mut j).is_err());
        assert!(s.reserve_income_v1(&p, 1_000 * BILI, h(96), bond(0), 1_000, 1, 1, PalwBudgetOriginV1::MarketFee, &mut j).is_err());
        assert_eq!((s.root_parts(), j.len()), before, "the queued full maximum still occupies R");
        s.confirm_native_income_payment_v1(&p, &h(95), 2_000, &mut j).unwrap();
        assert!(!s.claim_row(&h(95)).unwrap().live);
        s.release_due(2_099, &mut j);
        assert_eq!(s.bond_row(&bond(0)).unwrap().window.reward_sompi, 50 * BILI);
        s.release_due(2_100, &mut j);
        assert!(s.claim_row(&h(95)).is_none() && s.bond_row(&bond(0)).is_none());
        s.check_consistency().unwrap();
        let mut replay = empty.clone();
        for write in &j {
            if let PalwBudgetWriteV1::Row { table, key, old, new } = write {
                replay.apply_row(*table, key, old, new, false).unwrap();
            }
        }
        assert_eq!(replay.root_parts(), s.root_parts());
        for write in j.iter().rev() {
            if let PalwBudgetWriteV1::Row { table, key, old, new } = write {
                replay.apply_row(*table, key, old, new, true).unwrap();
            }
        }
        assert_eq!(replay.root_parts(), empty.root_parts());
    }

    #[test]
    fn collected_market_queue_collision_preserves_full_fee_and_journal() {
        let p = policy(1, false);
        let mut s = PalwBondBudgetStateV1::new(10, &p);
        let mut j = Vec::new();
        s.reserve_income_v1(&p, 1_000 * BILI, h(97), bond(0), 10, 5, 5, PalwBudgetOriginV1::MarketFee, &mut j).unwrap();
        s.reserve_income_v1(&p, 1_000 * BILI, h(98), bond(1), 10, 5, 5, PalwBudgetOriginV1::MarketFee, &mut j).unwrap();
        s.queue_collected_income_v1(&h(98), h(99), &mut j).unwrap();
        let before = (s.root_parts(), j.len());
        assert!(s.queue_collected_income_v1(&h(97), h(99), &mut j).is_err());
        assert_eq!((s.root_parts(), j.len()), before);
        assert_eq!(s.claim_row(&h(97)).unwrap().consumed.reward_sompi, 5);
        s.check_consistency().unwrap();
    }

    #[test]
    fn market_and_round_income_share_reward_room_with_issuance_without_manufacturing_claims_or_blocks() {
        let mut p = policy(1, false);
        p.max_open_claims_per_bond = 1;
        p.round_rights = PalwRoundRightsPolicyV1::ExecutionCap { rights_per_unit: 7 };
        let capital = 1_000 * BILI;
        let mut s = PalwBondBudgetStateV1::new(1, &p);
        let mut j = Vec::new();
        s.reserve_claim_v1(
            &p,
            capital,
            h(1),
            bond(0),
            None,
            10,
            PalwBudgetAskV1 { reward_sompi: 30 * BILI, ..Default::default() },
            PalwBudgetOriginV1::Attempt,
            &mut j,
        )
        .unwrap();
        // Open Q is already full. A real fee is monetary income, not another claim.
        s.reserve_income_v1(&p, capital, h(2), bond(0), 10, 15 * BILI, 10 * BILI, PalwBudgetOriginV1::MarketFee, &mut j).unwrap();
        s.reserve_income_v1(&p, capital, h(3), bond(0), 10, 5 * BILI, 5 * BILI, PalwBudgetOriginV1::RoundFee, &mut j).unwrap();
        let row = s.bond_row(&bond(0)).unwrap();
        assert_eq!(row.open_claims, 1);
        assert_eq!(row.window.claims, 1);
        assert_eq!(row.window.block_units, 0);
        assert_eq!(row.window.final_weight, 0);
        assert_eq!(row.window.round_rights, 0);
        assert_eq!(row.window.reward_sompi, 50 * BILI);
        assert_eq!(s.claim_row(&h(2)).unwrap().consumed.reward_sompi, 10 * BILI);
        assert!(!s.claim_row(&h(2)).unwrap().open && !s.claim_row(&h(2)).unwrap().live);
        let root = s.root_parts();
        let len = j.len();
        assert_eq!(
            s.reserve_income_v1(&p, capital, h(4), bond(0), 10, 1, 1, PalwBudgetOriginV1::MarketFee, &mut j),
            Err(PalwBudgetRefusalV1::Exhausted { dim: PalwBudgetDimV1::Reward })
        );
        assert_eq!((s.root_parts(), j.len()), (root, len));
        s.check_consistency().unwrap();
        s.release_due(109, &mut j);
        assert_eq!(s.bond_row(&bond(0)).unwrap().window.reward_sompi, 50 * BILI);
        s.release_due(110, &mut j);
        assert!(s.claim_row(&h(2)).is_none() && s.claim_row(&h(3)).is_none());
        s.reserve_income_v1(&p, capital, h(4), bond(0), 110, 50 * BILI, 50 * BILI, PalwBudgetOriginV1::MarketFee, &mut j).unwrap();
        s.check_consistency().unwrap();
    }

    #[test]
    fn fee_income_refuses_duplicate_unfunded_and_overflowing_promises_before_journal_writes() {
        let p = policy(1, false);
        let mut s = PalwBondBudgetStateV1::new(1, &p);
        let mut j = Vec::new();
        s.reserve_income_v1(&p, 1_000 * BILI, h(1), bond(0), 10, 2, 1, PalwBudgetOriginV1::MarketFee, &mut j).unwrap();
        for (id, daa, maximum, actual, origin) in [
            (h(1), 10, 2, 1, PalwBudgetOriginV1::MarketFee),
            (h(2), 10, 2, 3, PalwBudgetOriginV1::MarketFee),
            (h(2), 10, 2, 1, PalwBudgetOriginV1::KernelRoute),
            (h(2), u64::MAX, 2, 1, PalwBudgetOriginV1::RoundFee),
        ] {
            let root = s.root_parts();
            let len = j.len();
            assert!(s.reserve_income_v1(&p, 1_000 * BILI, id, bond(0), daa, maximum, actual, origin, &mut j).is_err());
            assert_eq!((s.root_parts(), j.len()), (root, len));
        }
        s.check_consistency().unwrap();
    }

    #[test]
    fn caps_read_capital_and_policy_only() {
        let p = policy(1, false);
        let caps = palw_bond_budget_caps_v1(&p, 13_000 * BILI);
        assert_eq!(caps.claims, 130);
        assert_eq!(caps.block_units, 52 * PALW_BUDGET_BLOCK_UNIT_V1);
        assert_eq!(caps.reward_sompi, 650 * BILI);
        assert_eq!(caps.final_weight, 5_200);
        // The signature itself is the proof the caps cannot read class, PWU, time, tokens or speed: (policy, capital) only. And the
        // ask never raises a cap: a reservation is admitted against the same caps whatever it asks.
        let r_honest = palw_bond_budget_reservation_v1(
            &p,
            PalwBudgetAskV1 { block_units: PALW_BUDGET_BLOCK_UNIT_V1, reward_sompi: 5 * BILI, final_weight: 40, liability_sompi: 0 },
        );
        let r_forged = palw_bond_budget_reservation_v1(
            &p,
            PalwBudgetAskV1 { block_units: PALW_BUDGET_BLOCK_UNIT_V1, reward_sompi: 5 * BILI, final_weight: 40, liability_sompi: 0 },
        );
        assert_eq!(r_honest, r_forged);
    }

    #[test]
    fn rho_moves_q_only() {
        let mut rng = Rng(0x5eed_0001);
        for _ in 0..2_000 {
            let capital = rng.below(1_000_000 * BILI);
            let base = palw_bond_budget_caps_v1(&policy(1, false), capital);
            for rho in [1u32, 100, 1_000] {
                for slice in [false, true] {
                    let caps = palw_bond_budget_caps_v1(&policy(rho, slice), capital);
                    assert_eq!(
                        (caps.block_units, caps.reward_sompi, caps.final_weight),
                        (base.block_units, base.reward_sompi, base.final_weight)
                    );
                    assert_eq!(caps.claims, (capital as u128 * 10 * rho as u128 / (1_000 * BILI) as u128) as u64);
                }
            }
        }
    }

    #[test]
    fn split_capital_never_raises_caps() {
        let mut rng = Rng(0x5eed_0002);
        for _ in 0..5_000 {
            let p = policy(1 + rng.below(1_000) as u32, rng.below(2) == 1);
            let total = rng.below(10_000_000 * BILI);
            let parts = 1 + rng.below(8);
            let mut left = total;
            let mut sum = PalwBudgetVectorV1::ZERO;
            for i in 0..parts {
                let take = if i + 1 == parts { left } else { rng.below(left + 1) };
                left -= take;
                sum = sum.checked_add(palw_bond_budget_caps_v1(&p, take)).unwrap();
            }
            assert!(sum.fits_within(&palw_bond_budget_caps_v1(&p, total)), "a split raised a cap: {sum:?}");
        }
    }

    #[test]
    fn slice_ceilings_fit_the_caps() {
        let mut rng = Rng(0x5eed_0003);
        for _ in 0..5_000 {
            let p = policy(1 + rng.below(1_000) as u32, true);
            let capital = rng.below(100_000_000 * BILI);
            let caps = palw_bond_budget_caps_v1(&p, capital);
            let (r, f) = palw_bond_budget_claim_ceilings_v1(&p).unwrap();
            assert!(caps.claims as u128 * r as u128 <= caps.reward_sompi as u128);
            assert!(caps.claims as u128 * f <= caps.final_weight);
        }
    }

    #[test]
    fn honest_and_forger_fill_to_the_same_ceilings() {
        // Same bond, same window, same rho: one producer asks the full reward of real work on every claim; the other forges fast with
        // asks that are larger still. Both stop at the same caps and neither admits past them.
        for rho in [1u32, 100, 1_000] {
            let p = policy(rho, true);
            let capital = 1_300 * BILI;
            let caps = palw_bond_budget_caps_v1(&p, capital);
            let mut totals = Vec::new();
            for ask_reward in [5 * BILI, 500 * BILI] {
                let mut s = PalwBondBudgetStateV1::new(0, &p);
                let mut j = Vec::new();
                let mut n = 0u64;
                loop {
                    let r = palw_bond_budget_reservation_v1(
                        &p,
                        PalwBudgetAskV1 { block_units: 0, reward_sompi: ask_reward, final_weight: 10_000, liability_sompi: 0 },
                    );
                    if s.reserve(&p, capital, h(n), bond(1), None, 0, r, PalwBudgetOriginV1::Rider, true, &mut j).is_err() {
                        break;
                    }
                    n += 1;
                }
                let w = s.bond_row(&bond(1)).unwrap().window;
                assert!(w.fits_within(&caps));
                assert_eq!(w.claims, n);
                totals.push((n, w.reward_sompi <= caps.reward_sompi, w.final_weight <= caps.final_weight));
            }
            assert!(totals.iter().all(|t| t.1 && t.2));
        }
    }

    #[test]
    fn release_only_at_d_plus_w_and_closing_returns_nothing() {
        let p = policy(1, false);
        let capital = 1_000 * BILI; // Q = 10
        let mut s = PalwBondBudgetStateV1::new(0, &p);
        let mut j = Vec::new();
        for i in 0..10 {
            let r = palw_bond_budget_reservation_v1(&p, PalwBudgetAskV1::default());
            s.reserve(&p, capital, h(i), bond(1), None, 5, r, PalwBudgetOriginV1::Attempt, true, &mut j).unwrap();
        }
        let r = palw_bond_budget_reservation_v1(&p, PalwBudgetAskV1::default());
        assert_eq!(
            s.clone().reserve(&p, capital, h(99), bond(1), None, 6, r, PalwBudgetOriginV1::Attempt, true, &mut j),
            Err(PalwBudgetRefusalV1::Exhausted { dim: PalwBudgetDimV1::Claims })
        );
        // Early Final / void / retry: every claim closes at once — the window does not move.
        for i in 0..10 {
            s.close(&h(i), &mut j);
        }
        s.release_due(104, &mut j);
        assert_eq!(s.bond_row(&bond(1)).unwrap().window.claims, 10, "closing returned room before d + W");
        assert!(s.window_holds(&bond(1), 104));
        assert!(s.clone().reserve(&p, capital, h(99), bond(1), None, 104, r, PalwBudgetOriginV1::Attempt, true, &mut j).is_err());
        // d + W = 105: released, and only then.
        s.release_due(105, &mut j);
        s.check_consistency().unwrap();
        assert!(s.bond_row(&bond(1)).is_none() && s.releases.is_empty());
        for i in 0..10 {
            s.forget(&h(i), &mut j);
        }
        assert!(s.claims.is_empty());
        assert!(!s.window_holds(&bond(1), 105));
        s.reserve(&p, capital, h(99), bond(1), None, 105, r, PalwBudgetOriginV1::Attempt, true, &mut j).unwrap();
    }

    #[test]
    fn consumption_never_exceeds_reservation() {
        let mut rng = Rng(0x5eed_0004);
        let p = policy(10, true);
        for _ in 0..500 {
            let mut s = PalwBondBudgetStateV1::new(0, &p);
            let mut j = Vec::new();
            let ask = PalwBudgetAskV1 {
                block_units: rng.below(3) * PALW_BUDGET_BLOCK_UNIT_V1,
                reward_sompi: rng.below(10 * BILI),
                final_weight: rng.below(100) as u128,
                liability_sompi: 0,
            };
            let r = palw_bond_budget_reservation_v1(&p, ask);
            s.reserve(&p, 1_000_000 * BILI, h(1), bond(1), None, 0, r, PalwBudgetOriginV1::FreePrompt, true, &mut j).unwrap();
            let mut paid = PalwBudgetVectorV1::ZERO;
            for _ in 0..20 {
                let dim = [PalwBudgetDimV1::BlockUnits, PalwBudgetDimV1::Reward, PalwBudgetDimV1::FinalWeight][rng.below(3) as usize];
                let want = rng.below(5 * BILI) as u128;
                let strict = rng.below(2) == 1;
                match s.consume(&h(1), dim, want, strict, &mut j).unwrap() {
                    Ok(g) => {
                        assert!(g <= want);
                        match dim {
                            PalwBudgetDimV1::BlockUnits => paid.block_units += g as u64,
                            PalwBudgetDimV1::Reward => paid.reward_sompi += g as u64,
                            _ => paid.final_weight += g,
                        }
                    }
                    Err(PalwBudgetRefusalV1::Short { .. }) => assert!(strict),
                    Err(e) => panic!("{e}"),
                }
            }
            assert!(paid.fits_within(&r), "paid {paid:?} beyond reserved {r:?}");
            s.close(&h(1), &mut j);
            assert_eq!(s.consume(&h(1), PalwBudgetDimV1::Reward, 1, false, &mut j), Some(Err(PalwBudgetRefusalV1::NotOpen)));
        }
    }

    #[test]
    fn legacy_rows_count_but_are_never_consumed() {
        let p = policy(1, false);
        let mut s = PalwBondBudgetStateV1::new(50, &p);
        let mut j = Vec::new();
        let ask = PalwBudgetVectorV1 {
            claims: 1,
            block_units: PALW_BUDGET_BLOCK_UNIT_V1,
            reward_sompi: 999 * BILI,
            final_weight: 1,
            round_rights: 0,
        };
        // A seed may overfill the window (old liabilities are counted, never refused) …
        s.reserve(&p, 1_000 * BILI, h(7), bond(1), None, 40, ask, PalwBudgetOriginV1::Legacy, false, &mut j).unwrap();
        // … the old claim is paid by its old rule …
        assert_eq!(s.consume(&h(7), PalwBudgetDimV1::Reward, 5, true, &mut j), None);
        // … and the bond admits nothing new until the seed leaves at 40 + W.
        let r = palw_bond_budget_reservation_v1(
            &p,
            PalwBudgetAskV1 { block_units: 0, reward_sompi: 1, final_weight: 1, liability_sompi: 0 },
        );
        assert!(s.clone().reserve(&p, 1_000 * BILI, h(8), bond(1), None, 60, r, PalwBudgetOriginV1::Attempt, true, &mut j).is_err());
        s.release_due(140, &mut j);
        s.close(&h(7), &mut j);
        s.reserve(&p, 1_000 * BILI, h(8), bond(1), None, 140, r, PalwBudgetOriginV1::Attempt, true, &mut j).unwrap();
    }

    #[test]
    fn rider_block_attribution_sums_to_one_block() {
        for n in 0..=64u64 {
            let (lead, each) = palw_rider_block_attribution_v1(n);
            assert_eq!(lead + each * n, PALW_BUDGET_BLOCK_UNIT_V1);
            assert!(lead >= each);
        }
    }

    #[test]
    fn shrink_is_same_block_only_and_never_below_consumption() {
        let p = policy(1, false);
        let mut s = PalwBondBudgetStateV1::new(0, &p);
        let mut j = Vec::new();
        let r = PalwBudgetVectorV1 {
            claims: 1,
            block_units: PALW_BUDGET_BLOCK_UNIT_V1,
            reward_sompi: 10 * BILI,
            final_weight: 30,
            round_rights: 0,
        };
        s.reserve(&p, 100_000 * BILI, h(1), bond(1), None, 9, r, PalwBudgetOriginV1::Attempt, true, &mut j).unwrap();
        let smaller = PalwBudgetVectorV1 { reward_sompi: 4 * BILI, block_units: 500_000, ..r };
        assert_eq!(s.clone().shrink_same_block(&h(1), smaller, 10, &mut j), Err(PalwBudgetRefusalV1::BadShrink));
        assert_eq!(
            s.clone().shrink_same_block(&h(1), PalwBudgetVectorV1 { reward_sompi: 11 * BILI, ..r }, 9, &mut j),
            Err(PalwBudgetRefusalV1::BadShrink)
        );
        s.shrink_same_block(&h(1), smaller, 9, &mut j).unwrap();
        assert_eq!(s.bond_row(&bond(1)).unwrap().window, smaller);
    }

    #[test]
    fn delta_round_trip_and_revert() {
        let p = policy(3, true);
        let ap = PalwModelAllocationPolicyV1 {
            version: PALW_MODEL_ALLOCATION_POLICY_VERSION_V2,
            epoch_daa: 10,
            seasoning_epochs: 1,
            curve: PalwAllocationCurveV1 { alpha_halves: 4 },
            max_models_per_bond: 4,
            average: None,
        };
        let mut s = PalwBondBudgetStateV1::new(0, &p);
        let before = s.clone();
        let mut j = Vec::new();
        s.assign(&ap, bond(1), &[(h(1), 300 * BILI)], 1, 1_000 * BILI, 0, |_| true, &mut j).unwrap();
        s.roll_epoch(&ap, 2, 20, |_| (1_000 * BILI, true), &mut j);
        s.accrue(50 * BILI, &mut j);
        assert_eq!(s.model_available(&h(1)), 50 * BILI, "one model holds the whole weight: all that accrued");
        let ask =
            PalwBudgetAskV1 { block_units: PALW_BUDGET_BLOCK_UNIT_V1, reward_sompi: 70 * BILI, final_weight: 9, liability_sompi: 0 };
        let r = s.reserve_claim_v1(&p, 1_000 * BILI, h(5), bond(1), Some(h(1)), 21, ask, PalwBudgetOriginV1::Attempt, &mut j).unwrap();
        // Sliced by rho = 3 (⌊50 BILI / 30⌋), and the model reserved exactly what the claim did.
        assert_eq!(r.reward_sompi, 50 * BILI / 30);
        assert_eq!(s.model_row(&h(1)).unwrap().reserved_sompi, r.reward_sompi);
        assert_eq!(s.claim_row(&h(5)).unwrap().model, Some((h(1), 2)));
        s.consume(&h(5), PalwBudgetDimV1::BlockUnits, PALW_BUDGET_BLOCK_UNIT_V1 as u128, true, &mut j).unwrap().unwrap();
        s.close(&h(5), &mut j);
        s.release_due(200, &mut j);
        // Replay forward onto `before` equals `s`; revert newest-first from `s` equals `before`.
        let mut fwd = before.clone();
        for w in &j {
            match w {
                PalwBudgetWriteV1::Row { table, key, old, new } => fwd.apply_row(*table, key, old, new, false).unwrap(),
                PalwBudgetWriteV1::Header { new, .. } => fwd.header = new.clone().unwrap(),
            }
        }
        assert_eq!(fwd, s);
        let mut back = s.clone();
        for w in j.iter().rev() {
            match w {
                PalwBudgetWriteV1::Row { table, key, old, new } => back.apply_row(*table, key, old, new, true).unwrap(),
                PalwBudgetWriteV1::Header { old, .. } => back.header = old.clone().unwrap(),
            }
        }
        assert_eq!(back, before);
        // A row that does not match its expectation is refused.
        if let Some(PalwBudgetWriteV1::Row { table, key, old, new }) = j.iter().find(|w| matches!(w, PalwBudgetWriteV1::Row { .. })) {
            assert!(back.apply_row(*table, key, new, old, false).is_err() || old == new);
        }
    }

    #[test]
    fn carriage_round_trip() {
        let p = policy(1, false);
        let mut s = PalwBondBudgetStateV1::new(3, &p);
        let mut j = Vec::new();
        s.reserve(
            &p,
            1_000 * BILI,
            h(1),
            bond(2),
            Some((h(4), 0)),
            3,
            palw_bond_budget_reservation_v1(&p, PalwBudgetAskV1::default()),
            PalwBudgetOriginV1::Attempt,
            true,
            &mut j,
        )
        .unwrap();
        let bytes = borsh::to_vec(&s).unwrap();
        let back: PalwBondBudgetStateV1 = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, s);
        assert_eq!(back.root_parts(), s.root_parts());
        let mut t = s.clone();
        t.close(&h(1), &mut j);
        assert_ne!(t.root_parts(), s.root_parts(), "every row moves the root");
    }

    // ---- ADR-0177 ----

    /// A TEST allocation at `α = halves / 2`.
    fn ap_alpha(seasoning: u32, halves: u8) -> PalwModelAllocationPolicyV1 {
        PalwModelAllocationPolicyV1 {
            version: PALW_MODEL_ALLOCATION_POLICY_VERSION_V2,
            epoch_daa: 10,
            seasoning_epochs: seasoning,
            curve: PalwAllocationCurveV1 { alpha_halves: halves },
            max_models_per_bond: 4,
            average: None,
        }
    }

    /// The TEST allocation the older rows use: the interim `α = 1.5` (unapproved).
    fn ap(seasoning: u32) -> PalwModelAllocationPolicyV1 {
        ap_alpha(seasoning, PALW_ALLOCATION_ALPHA_HALVES_INTERIM_V1)
    }

    fn w(n: u64) -> PalwAllocationWeightV1 {
        PalwAllocationWeightV1::from_u64(n)
    }

    #[test]
    fn curve_validation_isqrt_and_exact_powers() {
        for halves in 2..=PALW_ALLOCATION_ALPHA_HALVES_MAX_V1 {
            assert!(ap_alpha(1, halves).validate().is_ok());
        }
        assert!(PalwAllocationCurveV1 { alpha_halves: 1 }.validate().is_err(), "alpha >= 1");
        assert!(PalwAllocationCurveV1 { alpha_halves: 9 }.validate().is_err(), "alpha <= 4");
        assert_eq!(PalwModelAllocationPolicyV1::unapproved_probe_v1().curve.alpha_halves, 3, "the interim preset is alpha = 1.5");
        let mut old = ap(1);
        old.version = 1;
        assert!(old.validate().is_err(), "version 1 (the piecewise-linear f) is refused");
        // isqrt is the floor root: r² ≤ n < (r + 1)², on random and edge inputs up to 2^512 − 1.
        let mut rng = Rng(0x5eed_15c7);
        let one = w(1);
        let mut inputs = vec![PalwAllocationWeightV1::ZERO, one, w(2), w(3), w(4), w(15), w(16), w(17), PalwAllocationWeightV1::MAX];
        for _ in 0..500 {
            let limbs = 1 + rng.below(8) as usize;
            let mut x = PalwAllocationWeightV1::ZERO;
            for l in 0..limbs {
                x.0[l] = rng.next();
            }
            inputs.push(x);
        }
        for n in inputs {
            let r = palw_isqrt_u512_v1(n);
            assert!(r.bits() <= 256 && r.overflowing_mul(r).0 <= n, "r² ≤ n");
            // `overflowing_mul` drops (unflagged) the partial products past 512 bits, so square only below 2^256: at or above it
            // `(r + 1)² ≥ 2^512 > n` already.
            let r1 = r + one;
            assert!(r1.bits() > 256 || r1.overflowing_mul(r1).0 > n, "(r + 1)² > n");
        }
        let c = |halves| PalwAllocationCurveV1 { alpha_halves: halves };
        assert_eq!(c(2).weight(7), w(7));
        assert_eq!(c(4).weight(7), w(49));
        assert_eq!(c(3).weight(4), w(8), "4^1.5 = 8 exactly");
        assert_eq!(c(3).weight(2), w(2), "2^1.5 = 2.83, floored");
        assert_eq!(c(3).weight(1_000_000), w(1_000_000_000));
        assert_eq!(c(4).weight(3_000 * BILI as u128), PalwAllocationWeightV1::from_u128((3_000 * BILI as u128).pow(2)));
        assert_eq!(c(2).weight(0), PalwAllocationWeightV1::ZERO);
        // The largest S at the largest alpha: exact, no overflow.
        let s = w(u64::MAX);
        assert_eq!(c(8).weight(u64::MAX as u128), s * s * s * s);
        assert!(c(7).weight(u64::MAX as u128).bits() <= 224);
        // Monotone in S at every alpha.
        for halves in 2..=PALW_ALLOCATION_ALPHA_HALVES_MAX_V1 {
            let (mut last, mut s) = (PalwAllocationWeightV1::ZERO, 0u128);
            for _ in 0..1_000 {
                s += rng.below(u64::MAX / 2_000) as u128;
                let a = c(halves).weight(s);
                assert!(a >= last);
                last = a;
            }
        }
    }

    /// **The revision's headline: a 2:1 bond is a 4:1 allocation at `α = 2`** (before any cap), `2^1.5 = 2.83:1` at the interim `α = 1.5`
    /// and 2:1 at `α = 1` — exact, from the engine's own snapshot and availability.
    #[test]
    fn two_to_one_capital_is_four_to_one_at_alpha_two_and_two_to_one_at_alpha_one() {
        let s1 = 2_000 * BILI;
        let s2 = 1_000 * BILI;
        let r = 1_000 * BILI;
        // α = 1.5: ⌊S^1.5⌋ by isqrt, then ⌊R·A/ΣA⌋ — computed here independently in u128/f64-free integer steps.
        let a15 = |s: u64| {
            let cube = PalwAllocationWeightV1::from_u64(s) * s * s;
            palw_isqrt_u512_v1(cube)
        };
        let (x, y) = (a15(s1), a15(s2));
        let share = |a: PalwAllocationWeightV1| ((a * r).div_rem(x + y).0).as_u64();
        for (halves, expect) in [(2u8, (2 * r / 3, r / 3)), (3, (share(x), share(y))), (4, (4 * r / 5, r / 5))] {
            let a = ap_alpha(1, halves);
            let mut s = PalwBondBudgetStateV1::new(0, &policy(1, false));
            let mut j = Vec::new();
            s.assign(&a, bond(1), &[(h(1), s1)], 1, s1, 0, |_| true, &mut j).unwrap();
            s.assign(&a, bond(2), &[(h(2), s2)], 1, s2, 0, |_| true, &mut j).unwrap();
            s.roll_epoch(&a, 2, 20, |b| if *b == bond(1) { (s1, true) } else { (s2, true) }, &mut j);
            s.accrue(r, &mut j);
            let (big, small) = (s.model_available(&h(1)), s.model_available(&h(2)));
            assert_eq!((big, small), expect, "alpha = {halves}/2");
            assert!(big + small <= r, "within R_PALW; the remainder is never allocated");
            assert!(r - (big + small) < 2, "and only the floors are lost");
            if halves == 4 {
                assert_eq!(s.model_row(&h(1)).unwrap().weight, s.model_row(&h(2)).unwrap().weight * 4u64);
            }
            if halves == 3 {
                // 2^1.5 ≈ 2.8284: the big model's share is 2.8284 / 3.8284 of R.
                assert!(big.abs_diff(r * 28_284 / 38_284) <= r / 10_000, "2^1.5 : 1 ({big} : {small})");
            }
        }
    }

    /// **Σ R_m ≤ R_PALW** for random capitals up to the 64-bit limit, any number of models and every `α` (1 to 4 in half steps, the
    /// interim 1.5 among them); the zero-denominator rule.
    #[test]
    fn model_budgets_sum_within_accrued_and_zero_denominator_is_zero() {
        let mut rng = Rng(0x5eed_0006);
        for i in 0..2_000u64 {
            let halves = 2 + (i % 7) as u8;
            let curve = PalwAllocationCurveV1 { alpha_halves: halves };
            let accrued = rng.below(u64::MAX / 4);
            let models = 1 + rng.below(20) as usize;
            let weights: Vec<PalwAllocationWeightV1> = (0..models)
                .map(|_| curve.weight(if rng.below(4) == 0 { u64::MAX as u128 } else { rng.below(1 << 50) as u128 }))
                .collect();
            let sum = weights.iter().fold(PalwAllocationWeightV1::ZERO, |acc, w| acc + *w);
            let total: u128 = weights.iter().map(|w| palw_model_available_v1(accrued, w, &sum, 0) as u128).sum();
            assert!(total <= accrued as u128, "alpha = {halves}/2");
            if !sum.is_zero() {
                assert!(total + models as u128 > accrued as u128, "only the floors (< one per model) are lost");
            }
        }
        assert_eq!(palw_model_available_v1(1_000, &w(5), &PalwAllocationWeightV1::ZERO, 0), 0);
        assert_eq!(palw_model_available_v1(1_000, &PalwAllocationWeightV1::ZERO, &w(7), 0), 0);
    }

    /// **Splitting a model, or a bond, never raises the total** at any `α ≥ 1`: `A` is superadditive (`(x+y)^α ≥ x^α + y^α`) and the
    /// share `x / (x + B)` grows with `x`, so two models holding the same capital never out-earn one; and a bond's split is invisible
    /// (S_m is the capital, whoever holds it).
    #[test]
    fn splitting_a_model_or_a_bond_never_raises_the_total() {
        let mut rng = Rng(0x5eed_0177);
        for i in 0..600 {
            let alpha = 2 + (i % 7) as u8;
            let a = ap_alpha(1, alpha);
            let mine = 1 + rng.below(10_000) * BILI;
            let other = 1 + rng.below(10_000) * BILI;
            let cut = rng.below(mine);
            let accrued = rng.below(1 << 50);
            // `models`: (bond, model, amount).
            let run = |models: &[(u32, u64, u64)]| {
                let mut s = PalwBondBudgetStateV1::new(0, &policy(1, false));
                let mut j = Vec::new();
                for (b, m, amount) in models.iter().filter(|(_, _, x)| *x > 0) {
                    s.assign(&a, bond(*b), &[(h(*m), *amount)], 1, *amount, 0, |_| true, &mut j).unwrap();
                }
                let caps: BTreeMap<PalwBondKeyV2, u64> = models.iter().map(|(b, _, x)| (bond(*b), *x)).collect();
                s.roll_epoch(&a, 2, 20, |b| (caps.get(b).copied().unwrap_or(0), true), &mut j);
                s.accrue(accrued, &mut j);
                (s.model_available(&h(1)), s.model_available(&h(2)), s.model_available(&h(3)))
            };
            let (whole, rest, _) = run(&[(1, 1, mine), (9, 2, other)]);
            // A model split: the same capital over models 1 and 3.
            let (p1, rest_split, p3) = run(&[(1, 1, mine - cut), (2, 3, cut), (9, 2, other)]);
            assert!(p1 + p3 <= whole + 1, "alpha {alpha}: alpha_halves {alpha}: a model split never gains ({p1} + {p3} > {whole})");
            assert!(rest_split + 1 >= rest, "and the other model never loses to it");
            // A bond split: two bonds on one model are one model.
            let (b1, b_rest, _) = run(&[(1, 1, mine - cut), (2, 1, cut), (9, 2, other)]);
            assert_eq!((b1, b_rest), (whole, rest), "alpha_halves {alpha}: a bond split changes nothing");
        }
    }

    /// **A capped bond gains nothing more** (ADR-0176 caps unchanged): with the per-claim reward right binding, a model's larger share at
    /// `α = 2` leaves the reservation where `α = 1` put it, and once the bond's window is full no `α` admits another claim.
    #[test]
    fn a_capped_bond_gains_nothing_more_from_a_steeper_curve() {
        let p = policy(1, true);
        let (r_claim, _) = palw_bond_budget_claim_ceilings_v1(&p).unwrap();
        let run = |halves: u8| {
            let a = ap_alpha(1, halves);
            let mut s = PalwBondBudgetStateV1::new(0, &p);
            let mut j = Vec::new();
            s.assign(&a, bond(1), &[(h(1), 3_000 * BILI)], 1, 3_000 * BILI, 0, |_| true, &mut j).unwrap();
            s.assign(&a, bond(2), &[(h(2), 1_000 * BILI)], 1, 1_000 * BILI, 0, |_| true, &mut j).unwrap();
            s.roll_epoch(&a, 2, 20, |b| if *b == bond(1) { (3_000 * BILI, true) } else { (1_000 * BILI, true) }, &mut j);
            s.accrue(1_000_000 * BILI, &mut j);
            let share = s.model_available(&h(1));
            let ask = PalwBudgetAskV1 {
                block_units: PALW_BUDGET_BLOCK_UNIT_V1,
                reward_sompi: u64::MAX / 2,
                final_weight: 1,
                liability_sompi: 0,
            };
            let mut admitted = Vec::new();
            for i in 0..1_000u64 {
                match s.reserve_claim_v1(
                    &p,
                    3_000 * BILI,
                    h(100 + i),
                    bond(1),
                    Some(h(1)),
                    21,
                    ask,
                    PalwBudgetOriginV1::Attempt,
                    &mut j,
                ) {
                    Ok(r) => admitted.push(r.reward_sompi),
                    Err(_) => break,
                }
            }
            (share, admitted)
        };
        let (share1, linear) = run(2);
        let (share15, interim) = run(3);
        let (share2, square) = run(4);
        assert!(share15 > share1 && share2 > share15);
        assert_eq!(linear, interim, "alpha = 1.5 gains a capped bond nothing either");
        assert!(share2 > share1, "the steeper curve gives the larger model more of the budget ({share2} vs {share1})");
        assert_eq!(linear, square, "but a capped bond is reserved exactly the same claims and reward");
        assert!(linear.iter().all(|r| *r == r_claim), "each at its per-claim reward right");
        assert_eq!(linear.len() as u64, palw_bond_budget_caps_v1(&p, 3_000 * BILI).block_units / PALW_BUDGET_BLOCK_UNIT_V1);
    }

    #[test]
    fn many_claims_do_not_multiply_s_m_and_assignments_never_exceed_capital() {
        let a = ap(1);
        let mut s = PalwBondBudgetStateV1::new(0, &policy(1_000, true));
        let mut j = Vec::new();
        assert!(s.assign(&a, bond(1), &[(h(1), 600 * BILI), (h(2), 600 * BILI)], 1, 1_000 * BILI, 0, |_| true, &mut j).is_err());
        assert!(s.assign(&a, bond(1), &[(h(2), 1), (h(1), 1)], 1, 1_000 * BILI, 0, |_| true, &mut j).is_err(), "ascending");
        assert!(s.assign(&a, bond(1), &[(h(1), 1)], 1, 1_000 * BILI, 0, |m| *m != h(1), &mut j).is_err(), "registered");
        s.assign(&a, bond(1), &[(h(1), 600 * BILI), (h(2), 400 * BILI)], 1, 1_000 * BILI, 0, |_| true, &mut j).unwrap();
        assert!(s.assign(&a, bond(1), &[(h(1), 1)], 1, 1_000 * BILI, 0, |_| true, &mut j).is_err(), "replay");
        s.roll_epoch(&a, 2, 20, |_| (1_000 * BILI, true), &mut j);
        // A thousand claims change nothing: capital is read from the assignment, never from reservations.
        let p = policy(1_000, true);
        for i in 0..1_000 {
            let r = palw_bond_budget_reservation_v1(&p, PalwBudgetAskV1::default());
            let _ = s.reserve(&p, 1_000 * BILI, h(100 + i), bond(1), Some((h(1), 2)), 20, r, PalwBudgetOriginV1::Rider, true, &mut j);
        }
        assert_eq!(s.model_row(&h(1)).unwrap().capital, 600 * BILI as u128);
        assert_eq!(s.model_row(&h(2)).unwrap().capital, 400 * BILI as u128);
    }

    #[test]
    fn seasoning_delays_increases_not_decreases_and_pro_rata_after_slash() {
        let a = ap(1);
        let mut s = PalwBondBudgetStateV1::new(0, &policy(1, false));
        let mut j = Vec::new();
        s.assign(&a, bond(1), &[(h(1), 800 * BILI)], 1, 1_000 * BILI, 0, |_| true, &mut j).unwrap();
        s.roll_epoch(&a, 1, 10, |_| (1_000 * BILI, true), &mut j);
        assert!(s.model_row(&h(1)).is_none(), "an increase waits a full epoch");
        s.roll_epoch(&a, 2, 20, |_| (1_000 * BILI, true), &mut j);
        assert_eq!(s.model_row(&h(1)).unwrap().capital, 800 * BILI as u128);
        // A decrease (and a move) applies at once: the moved capital counts nowhere while it seasons.
        s.assign(&a, bond(1), &[(h(1), 100 * BILI), (h(2), 700 * BILI)], 2, 1_000 * BILI, 2, |_| true, &mut j).unwrap();
        s.roll_epoch(&a, 3, 30, |_| (1_000 * BILI, true), &mut j);
        assert_eq!(s.model_row(&h(1)).unwrap().capital, 100 * BILI as u128);
        assert!(s.model_row(&h(2)).is_none());
        s.roll_epoch(&a, 4, 40, |_| (1_000 * BILI, true), &mut j);
        assert_eq!(s.model_row(&h(2)).unwrap().capital, 700 * BILI as u128);
        // A slash to 400: pro rata (100:700 → 50:350).
        s.roll_epoch(&a, 5, 50, |_| (400 * BILI, true), &mut j);
        assert_eq!(s.model_row(&h(1)).unwrap().capital, 50 * BILI as u128);
        assert_eq!(s.model_row(&h(2)).unwrap().capital, 350 * BILI as u128);
        // Retiring: nothing.
        s.roll_epoch(&a, 6, 60, |_| (400 * BILI, false), &mut j);
        assert!(s.models.is_empty());
        assert!(s.assignment_row(&bond(1)).is_some(), "the sequence survives");
    }

    #[test]
    fn same_s_m_same_allocation_whoever_owns_it() {
        // One bond with 1,000 on model 1, or four bonds with 250 each: the same S_m, the same A_m and the same share.
        let a = ap(1);
        let run = |owners: &[(u32, u64)]| {
            let mut s = PalwBondBudgetStateV1::new(0, &policy(1, false));
            let mut j = Vec::new();
            for (b, amount) in owners {
                s.assign(&a, bond(*b), &[(h(1), *amount)], 1, 10_000 * BILI, 0, |_| true, &mut j).unwrap();
            }
            s.assign(&a, bond(99), &[(h(2), 3_000 * BILI)], 1, 10_000 * BILI, 0, |_| true, &mut j).unwrap();
            s.roll_epoch(&a, 2, 20, |_| (10_000 * BILI, true), &mut j);
            s.accrue(1_000 * BILI, &mut j);
            (s.model_row(&h(1)).copied(), s.model_available(&h(1)), s.model_available(&h(2)))
        };
        assert_eq!(run(&[(1, 1_000 * BILI)]), run(&[(1, 250 * BILI), (2, 250 * BILI), (3, 250 * BILI), (4, 250 * BILI)]));
    }

    #[test]
    fn model_budget_clips_reward_and_never_restores_the_bond_window() {
        let a = ap(1);
        let p = policy(1, false);
        let mut s = PalwBondBudgetStateV1::new(0, &p);
        let mut j = Vec::new();
        s.assign(&a, bond(1), &[(h(1), 1_000 * BILI)], 1, 1_000 * BILI, 0, |_| true, &mut j).unwrap();
        s.roll_epoch(&a, 2, 20, |_| (1_000 * BILI, true), &mut j);
        s.accrue(3 * BILI, &mut j);
        let grant = s.reserve_model_reward(&h(1), 5 * BILI, &mut j);
        assert_eq!(grant, 3 * BILI);
        assert_eq!(s.model_available(&h(1)), 0, "the model is spent until more accrues");
        let r = palw_bond_budget_reservation_v1(
            &p,
            PalwBudgetAskV1 { block_units: 0, reward_sompi: grant, final_weight: 7, liability_sompi: 0 },
        );
        s.reserve(&p, 1_000 * BILI, h(9), bond(1), Some((h(1), 2)), 21, r, PalwBudgetOriginV1::Attempt, true, &mut j).unwrap();
        let window = s.bond_row(&bond(1)).unwrap().window;
        // More capital on the model and a new epoch: the bond's window is untouched.
        s.assign(&a, bond(2), &[(h(1), 9_000 * BILI)], 1, 9_000 * BILI, 2, |_| true, &mut j).unwrap();
        s.roll_epoch(&a, 4, 40, |_| (9_000 * BILI, true), &mut j);
        assert_eq!(s.bond_row(&bond(1)).unwrap().window, window);
        // An unknown model, or none assigned: zero.
        assert_eq!(s.reserve_model_reward(&h(77), 1, &mut j), 0);
    }

    #[test]
    fn policies_validate_and_probes_are_well_formed() {
        assert!(PalwBondBudgetPolicyV1::unapproved_probe_v1().validate().is_ok());
        assert!(PalwModelAllocationPolicyV1::unapproved_probe_v1().validate().is_ok());
        let valid = PalwBondBudgetPolicyV1 { window_daa: 300, ..policy(1, false) };
        assert!(valid.validate().is_ok());
        let bad = |f: &dyn Fn(&mut PalwBondBudgetPolicyV1)| {
            let mut p = valid.clone();
            f(&mut p);
            p.validate().is_err()
        };
        assert!(bad(&|p| p.rho = 0));
        assert!(bad(&|p| {
            p.claims_per_unit = u64::MAX;
            p.rho = 2
        }));
        assert!(bad(&|p| p.version = 1), "version 1 is refused");
        // PESG §6: W within ECON's range, at the edges.
        assert!(bad(&|p| p.window_daa = PALW_BOND_BUDGET_W_MIN_DAA_V1 - 1));
        assert!(!bad(&|p| p.window_daa = PALW_BOND_BUDGET_W_MIN_DAA_V1));
        assert!(!bad(&|p| p.window_daa = PALW_BOND_BUDGET_W_MAX_DAA_V1));
        assert!(bad(&|p| p.window_daa = PALW_BOND_BUDGET_W_MAX_DAA_V1 + 1));
        assert!(bad(&|p| p.liability_hold_daa = 0));
        assert!(!bad(&|p| p.export_cap_permille = 510));
        assert!(bad(&|p| p.export_cap_permille = 511), "the export cap is at most 0.51");
        assert!(bad(&|p| p.weight_value = PalwWeightValuePolicyV1::SompiPerWeight { sompi_per_weight: 0 }));
        let probe = PalwBondBudgetPolicyV1::unapproved_probe_v1();
        assert_eq!((probe.window_daa, probe.liability_hold_daa, probe.weight_value), (280, 280, PalwWeightValuePolicyV1::Unknown));
        assert_ne!(policy(1, false).digest(), policy(2, false).digest());
    }

    // ---- PESG §6: the safety bounds, independent of economics ----

    /// **Open claims per bond ≤ ⌊C/K⌋**, at the edge: seven claims of `K = C/7` fit, the eighth is refused; closing one (its Final)
    /// admits one more while the liability hold keeps the capital held.
    #[test]
    fn open_claims_stop_at_capital_over_reservation() {
        let p = policy(1, false);
        let capital = 7_000 * BILI;
        let k = (capital / 7) as u128;
        let mut s = PalwBondBudgetStateV1::new(0, &p);
        let mut j = Vec::new();
        let ask = PalwBudgetAskV1 { block_units: 0, reward_sompi: 0, final_weight: 0, liability_sompi: k };
        for i in 0..7u64 {
            s.reserve_claim_v1(&p, capital, h(i), bond(1), None, 1, ask, PalwBudgetOriginV1::Attempt, &mut j).unwrap();
        }
        assert_eq!(
            s.preview_claim_v1(&p, capital, bond(1), None, ask),
            Err(PalwBudgetRefusalV1::LiabilityExceedsCapital),
            "the eighth"
        );
        // One unit under the edge still fits; the edge itself is the capital.
        let small = PalwBudgetAskV1 { liability_sompi: 0, ..ask };
        assert!(s.preview_claim_v1(&p, capital, bond(1), None, small).is_ok(), "a claim reserving nothing is not bounded by K");
        s.close_at(&h(0), Some(1 + p.liability_hold_daa), &mut j);
        assert!(s.liability_holds(&bond(1), 2) && s.withdrawal_holds(&bond(1), 2));
        s.reserve_claim_v1(&p, capital, h(7), bond(1), None, 2, ask, PalwBudgetOriginV1::Attempt, &mut j).unwrap();
        assert_eq!(s.bond_row(&bond(1)).unwrap().open_liability_sompi, 7 * k);
        s.check_consistency().unwrap();
    }

    /// **The unsettled (pre-Final) weight per bond ≤ F_max**, at the edge — and it outlives the window: a claim still open past `d + W`
    /// leaves the window (its F returns to the issuance budget) but not the unsettled weight, so a new claim is refused until one is
    /// terminal.
    #[test]
    fn unsettled_weight_stops_at_the_final_weight_cap_even_past_the_window() {
        let p = policy(1, false);
        let capital = 1_000 * BILI;
        let f_max = palw_bond_budget_caps_v1(&p, capital).final_weight;
        let mut s = PalwBondBudgetStateV1::new(0, &p);
        let mut j = Vec::new();
        let ask = |f: u128| PalwBudgetAskV1 { block_units: 0, reward_sompi: 0, final_weight: f, liability_sompi: 0 };
        s.reserve_claim_v1(&p, capital, h(1), bond(1), None, 1, ask(f_max - 1), PalwBudgetOriginV1::Attempt, &mut j).unwrap();
        s.reserve_claim_v1(&p, capital, h(2), bond(1), None, 1, ask(1), PalwBudgetOriginV1::Attempt, &mut j).unwrap();
        assert_eq!(s.unsettled_final_weight(&bond(1)), f_max, "at the edge");
        assert_eq!(s.preview_claim_v1(&p, capital, bond(1), None, ask(1)), Err(PalwBudgetRefusalV1::UnsettledWeight));
        // Past d + W both rows leave the window, still open: the window is empty, the unsettled weight is not.
        s.release_due(1 + p.window_daa, &mut j);
        assert!(s.bond_row(&bond(1)).unwrap().window.is_zero());
        assert_eq!(s.preview_claim_v1(&p, capital, bond(1), None, ask(1)), Err(PalwBudgetRefusalV1::UnsettledWeight));
        s.close_at(&h(2), Some(200), &mut j);
        assert!(s.preview_claim_v1(&p, capital, bond(1), None, ask(1)).is_ok(), "one terminal claim frees its weight");
        assert_eq!(s.preview_claim_v1(&p, capital, bond(1), None, ask(2)), Err(PalwBudgetRefusalV1::UnsettledWeight));
        s.check_consistency().unwrap();
    }

    /// **The value-of-weight slot**: `Unknown` bounds weight by `F_max` alone; a known value holds `reward + F·v ≤ R_max`, at the edge.
    #[test]
    fn a_known_value_of_weight_shares_the_reward_budget() {
        let mut p = policy(1, false);
        let capital = 1_000 * BILI;
        let caps = palw_bond_budget_caps_v1(&p, capital);
        let ask = |r: u64, f: u128| PalwBudgetAskV1 { block_units: 0, reward_sompi: r, final_weight: f, liability_sompi: 0 };
        let s = PalwBondBudgetStateV1::new(0, &p);
        assert!(
            s.preview_claim_v1(&p, capital, bond(1), None, ask(caps.reward_sompi, caps.final_weight)).is_ok(),
            "Unknown: separate"
        );
        p.weight_value = PalwWeightValuePolicyV1::SompiPerWeight { sompi_per_weight: 1_000 };
        let r = caps.reward_sompi - 1_000 * 7;
        assert!(s.preview_claim_v1(&p, capital, bond(1), None, ask(r, 7)).is_ok(), "R + F·v = R_max");
        assert_eq!(
            s.preview_claim_v1(&p, capital, bond(1), None, ask(r + 1, 7)),
            Err(PalwBudgetRefusalV1::Exhausted { dim: PalwBudgetDimV1::Reward })
        );
    }

    /// **The liability hold `H_L` is its own clock**: a terminal claim holds its bond's capital until `terminal + H_L` whatever the
    /// issuance window says; the hold only extends; it ends by its release entry; deltas carry it.
    #[test]
    fn the_liability_hold_is_apart_from_the_issuance_window() {
        let p = policy(1, false);
        let capital = 1_000 * BILI;
        let mut s = PalwBondBudgetStateV1::new(0, &p);
        let before = s.clone();
        let mut j = Vec::new();
        let ask = PalwBudgetAskV1 { block_units: 0, reward_sompi: 1, final_weight: 1, liability_sompi: 10 };
        s.reserve_claim_v1(&p, capital, h(1), bond(1), None, 10, ask, PalwBudgetOriginV1::Attempt, &mut j).unwrap();
        s.reserve_claim_v1(&p, capital, h(2), bond(1), None, 10, ask, PalwBudgetOriginV1::Attempt, &mut j).unwrap();
        s.close_at(&h(1), Some(20 + p.liability_hold_daa), &mut j);
        s.close_at(&h(2), Some(15 + p.liability_hold_daa), &mut j);
        assert_eq!(s.bond_row(&bond(1)).unwrap().liability_hold_until, 50, "the hold only extends");
        assert_eq!(s.liability_releases.len(), 1);
        // The window (W = 100) outlasts the hold here: they are separate readers.
        s.release_due(49, &mut j);
        assert!(s.liability_holds(&bond(1), 49) && s.window_holds(&bond(1), 49));
        s.release_due(50, &mut j);
        assert!(!s.liability_holds(&bond(1), 50) && s.window_holds(&bond(1), 50) && s.withdrawal_holds(&bond(1), 50));
        s.release_due(110, &mut j);
        assert!(!s.withdrawal_holds(&bond(1), 110));
        s.check_consistency().unwrap();
        // Deltas: forward from `before` is `s`, newest-first back is `before`.
        let mut fwd = before.clone();
        for w in &j {
            match w {
                PalwBudgetWriteV1::Row { table, key, old, new } => fwd.apply_row(*table, key, old, new, false).unwrap(),
                PalwBudgetWriteV1::Header { new, .. } => fwd.header = new.clone().unwrap(),
            }
        }
        assert_eq!(fwd, s);
        let mut back = s.clone();
        for w in j.iter().rev() {
            match w {
                PalwBudgetWriteV1::Row { table, key, old, new } => back.apply_row(*table, key, old, new, true).unwrap(),
                PalwBudgetWriteV1::Header { old, .. } => back.header = old.clone().unwrap(),
            }
        }
        assert_eq!(back, before);
    }

    /// **The external export cap**: `⌊0.51 · held⌋`, floored, never above 0.51 whatever the policy says.
    #[test]
    fn the_export_cap_is_at_most_fifty_one_percent_of_the_held_collateral() {
        let p = policy(1, false);
        assert_eq!(palw_bond_budget_export_cap_v1(&p, 1_000), 510);
        assert_eq!(palw_bond_budget_export_cap_v1(&p, 999), 509, "floored");
        assert_eq!(palw_bond_budget_export_cap_v1(&p, 0), 0, "nothing held: nothing exported");
        let lower = PalwBondBudgetPolicyV1 { export_cap_permille: 100, ..p.clone() };
        assert_eq!(palw_bond_budget_export_cap_v1(&lower, 1_000), 100);
        let bent = PalwBondBudgetPolicyV1 { export_cap_permille: 900, ..p };
        assert_eq!(palw_bond_budget_export_cap_v1(&bent, 1_000), 510, "an unvalidated policy is still held to 0.51");
    }

    #[test]
    fn consume_block_for_bond_draws_oldest_first_all_or_nothing() {
        let p = policy(1, false);
        let mut s = PalwBondBudgetStateV1::new(0, &p);
        let mut j = Vec::new();
        let half = PalwBudgetVectorV1 {
            claims: 1,
            block_units: PALW_BUDGET_BLOCK_UNIT_V1 / 2,
            reward_sompi: 0,
            final_weight: 0,
            round_rights: 0,
        };
        s.reserve(&p, 100_000 * BILI, h(1), bond(1), None, 1, half, PalwBudgetOriginV1::Rider, true, &mut j).unwrap();
        assert!(s.consume_block_for_bond(&bond(1), &mut j).is_err());
        assert_eq!(s.claim_row(&h(1)).unwrap().consumed.block_units, 0, "all or nothing");
        s.reserve(&p, 100_000 * BILI, h(2), bond(1), None, 2, half, PalwBudgetOriginV1::Rider, true, &mut j).unwrap();
        s.consume_block_for_bond(&bond(1), &mut j).unwrap();
        assert_eq!(s.claim_row(&h(1)).unwrap().consumed.block_units, PALW_BUDGET_BLOCK_UNIT_V1 / 2);
        assert_eq!(s.claim_row(&h(2)).unwrap().consumed.block_units, PALW_BUDGET_BLOCK_UNIT_V1 / 2);
    }

    // ---- the fences ----

    #[test]
    fn both_fences_are_dormant_some_only_hashed_and_refused_when_armed() {
        use crate::config::params::{
            DEVNET_PARAMS, MAINNET_PARAMS, SIMNET_PARAMS, TESTNET_PARAMS, palw_rc_shipped_params, palw_t12_shipped_params,
        };
        for p in [MAINNET_PARAMS, TESTNET_PARAMS, DEVNET_PARAMS, SIMNET_PARAMS, palw_rc_shipped_params(), palw_t12_shipped_params()] {
            assert!(p.palw_bond_budget_v1.is_none() && p.palw_model_bond_allocation_v1.is_none());
            assert!(!p.palw_bond_budget_active_at(u64::MAX) && !p.palw_model_bond_allocation_active_at(u64::MAX));
            assert!(p.palw_fences_v1().iter().any(|(n, f)| *n == "palw_bond_budget_v1" && f.is_none()));
            assert!(p.palw_fences_v1().iter().any(|(n, f)| *n == "palw_model_bond_allocation_v1" && f.is_none()));
            // Some-only: scheduling a future height moves the params and schedule ids (the policy hashed with it) and leaves the
            // handshake identity alone (the height collapses to never(), and never() to absent).
            let mut future = p.clone();
            future.palw_bond_budget_v1 = Some(PalwBondBudgetFenceV1::at(ForkActivation::new(9_000_000)));
            future.palw_model_bond_allocation_v1 = Some(PalwModelBondAllocationFenceV1::at(ForkActivation::new(9_000_001)));
            assert_ne!(p.consensus_params_id(), future.consensus_params_id());
            assert_ne!(p.consensus_schedule_id(), future.consensus_schedule_id());
            assert_eq!(p.consensus_identity_id(), future.consensus_identity_id());
            let mut other = future.clone();
            other.palw_bond_budget_v1.as_mut().unwrap().policy.window_daa = 2;
            assert_ne!(future.consensus_params_id(), other.consensus_params_id(), "the policy rides the fence's hash");
            let mut never = p.clone();
            never.palw_bond_budget_v1 = Some(PalwBondBudgetFenceV1::at(ForkActivation::never()));
            never.palw_model_bond_allocation_v1 = Some(PalwModelBondAllocationFenceV1::at(ForkActivation::never()));
            assert_eq!(p.consensus_identity_id(), never.consensus_identity_id(), "never() collapses whole");
            assert!(never.validate_palw_bond_budget_v1().is_ok() && never.validate_palw_model_bond_allocation_v1().is_ok());
        }
        let mut armed = palw_t12_shipped_params();
        armed.palw_bond_budget_v1 = Some(PalwBondBudgetFenceV1::at(ForkActivation::new(9_000_000)));
        assert!(armed.validate_palw_bond_budget_v1().is_err(), "an armed budget is refused");
        assert!(armed.validate_palw_v2().is_err(), "and validate_palw_v2 says so");
        let mut malformed = armed.clone();
        malformed.palw_bond_budget_v1.as_mut().unwrap().policy.rho = 0;
        let PalwModeV2Error::Invalid(why) = malformed.validate_palw_bond_budget_v1().unwrap_err() else { panic!("Invalid") };
        assert!(why.contains("policy is invalid"), "a malformed policy is named before the arming refusal: {why}");
        let mut alone = palw_t12_shipped_params();
        alone.palw_model_bond_allocation_v1 = Some(PalwModelBondAllocationFenceV1::at(ForkActivation::new(9_000_000)));
        let PalwModeV2Error::Invalid(why) = alone.validate_palw_model_bond_allocation_v1().unwrap_err() else { panic!("Invalid") };
        assert!(why.contains("requires palw_bond_budget_v1"), "{why}");
        let mut both = armed.clone();
        both.palw_model_bond_allocation_v1 = Some(PalwModelBondAllocationFenceV1::at(ForkActivation::new(9_000_000)));
        assert!(both.validate_palw_model_bond_allocation_v1().is_err(), "an armed allocation is refused even over the budget");
    }

    #[test]
    fn reporter_monetary_fence_is_separate_dormant_hashed_and_requires_budget_before_it() {
        use crate::config::params::{
            DEVNET_PARAMS, MAINNET_PARAMS, SIMNET_PARAMS, TESTNET_PARAMS, palw_rc_shipped_params, palw_t12_shipped_params,
        };
        for params in
            [MAINNET_PARAMS, TESTNET_PARAMS, DEVNET_PARAMS, SIMNET_PARAMS, palw_rc_shipped_params(), palw_t12_shipped_params()]
        {
            assert!(params.palw_reporter_income_v2.is_none());
            assert!(!params.palw_reporter_income_active_at(u64::MAX));
            assert!(params.validate_palw_reporter_income_v2().is_ok());
            assert!(params.palw_fences_v1().iter().any(|(name, at)| *name == "palw_reporter_income_v2" && at.is_none()));
            let mut future = params.clone();
            future.palw_reporter_income_v2 = Some(ForkActivation::new(9_000_001));
            assert_ne!(params.consensus_params_id(), future.consensus_params_id());
            assert_ne!(params.consensus_schedule_id(), future.consensus_schedule_id());
            assert_eq!(params.consensus_identity_id(), future.consensus_identity_id());
            assert!(future.validate_palw_reporter_income_v2().is_err(), "reporter fence without earlier budget refuses");
            future.palw_bond_budget_v1 = Some(PalwBondBudgetFenceV1::at(ForkActivation::new(9_000_002)));
            let PalwModeV2Error::Invalid(why) = future.validate_palw_reporter_income_v2().unwrap_err() else { panic!("Invalid") };
            assert!(why.contains("at or below"));
            future.palw_bond_budget_v1.as_mut().unwrap().activation = ForkActivation::new(9_000_000);
            let PalwModeV2Error::Invalid(why) = future.validate_palw_reporter_income_v2().unwrap_err() else { panic!("Invalid") };
            assert!(why.contains("cannot be armed"));
            future.sync_palw_bond_budget_v1();
            if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &future.palw_consensus_mode {
                assert!(!bundle.state.reporter_income_active_at(9_000_000));
                assert!(bundle.state.reporter_income_active_at(9_000_001));
            }
            let mut never = params.clone();
            never.palw_reporter_income_v2 = Some(ForkActivation::never());
            assert_eq!(params.consensus_identity_id(), never.consensus_identity_id());
            assert!(never.validate_palw_reporter_income_v2().is_ok());
        }
    }

    #[test]
    fn the_mirror_carries_both_fences_and_the_epoch_clock() {
        let mirror = PalwBondBudgetMirrorV1 { from_daa: 100, policy: policy(1, false), allocation: Some((150, ap(1))) };
        assert!(!mirror.active_at(99) && mirror.active_at(100));
        assert!(mirror.allocation_at(149).is_none() && mirror.allocation_at(150).is_some());
        assert_eq!(mirror.allocation_epoch_at(149), None);
        assert_eq!(mirror.allocation_epoch_at(150), Some(0));
        assert_eq!(mirror.allocation_epoch_at(169), Some(1));
    }

    // ---- readiness §3e: Round rights capped before the draw ----

    use crate::palw_execution_lane_v1::PalwExecFinalV1;
    use crate::palw_execution_quanta_v1::{PALW_EXECUTION_QUANTUM_V1, palw_execution_mint_quanta_windowed_capped_v1};

    fn exec_final(claim: u64, root: u64, credit: u64, b: u32) -> PalwExecFinalV1 {
        PalwExecFinalV1 {
            domain: h(1_000 + b as u64),
            bond: bond(b),
            operator_id: h(2_000 + b as u64),
            claim_id: h(claim),
            execution_root: h(root),
            credit,
            accepted_blue_score: 0,
        }
    }

    fn per_bond(quanta: &[crate::palw_execution_quanta_v1::PalwExecQuantumV1]) -> BTreeMap<PalwBondKeyV2, u64> {
        let mut out = BTreeMap::new();
        for q in quanta {
            *out.entry(q.bond).or_insert(0u64) += 1;
        }
        out
    }

    fn draw(
        finals: &[PalwExecFinalV1],
        seed: u64,
        window: u64,
        remaining: &dyn Fn(&PalwBondKeyV2) -> u64,
    ) -> BTreeMap<PalwBondKeyV2, u64> {
        per_bond(&palw_execution_mint_quanta_windowed_capped_v1(
            finals,
            h(seed),
            PALW_EXECUTION_QUANTUM_V1 as u128,
            0,
            window,
            &BTreeSet::new(),
            remaining,
        ))
    }

    /// **WORK**: the same bond with light and heavy verified work — below the cap the candidates follow the work (3 vs 30 tickets of
    /// 100,000 credit units), at the cap they stop at the bond's remaining rights.
    #[test]
    fn candidates_follow_work_below_the_cap_and_stop_at_it() {
        let finals = [exec_final(1, 11, 300_000, 1), exec_final(2, 12, 3_000_000, 2)];
        let free = draw(&finals, 7, 10_000, &|_| 1_000);
        assert_eq!((free[&bond(1)], free[&bond(2)]), (3, 30), "proportional to verified work below the cap");
        let capped = draw(&finals, 7, 10_000, &|b| if *b == bond(2) { 10 } else { 1_000 });
        assert_eq!((capped[&bond(1)], capped[&bond(2)]), (3, 10), "the heavy job stops at its bond's remaining rights");
        let none = draw(&finals, 7, 10_000, &|_| 0);
        assert!(none.is_empty(), "no remaining rights, no candidate: a capped bond floods nothing");
    }

    /// **WINDOW**: a saturated shared window (120) with a huge candidate set allocates exactly 120, never more than a bond's remaining;
    /// concurrent draws in two spans, through the engine, never pass the bond's cap, and the room returns only at `draw + W`.
    #[test]
    fn the_window_is_shared_and_concurrent_draws_never_pass_the_cap() {
        let finals: Vec<_> = (0..40u64).map(|i| exec_final(100 + i, 200 + i, 50_000_000, (i % 8) as u32 + 1)).collect();
        let caps: BTreeMap<PalwBondKeyV2, u64> = (1..=8u32).map(|b| (bond(b), 20 * b as u64)).collect();
        let allocated = draw(&finals, 9, 120, &|b| caps[b]);
        assert_eq!(allocated.values().sum::<u64>(), 120, "the shared window, not a per-claim grant");
        assert!(allocated.iter().all(|(b, n)| *n <= caps[b]), "{allocated:?}");
        // Concurrent windows through the engine (ExecutionCap: 30 tickets per 1,000 BILI).
        let p =
            PalwBondBudgetPolicyV1 { round_rights: PalwRoundRightsPolicyV1::ExecutionCap { rights_per_unit: 30 }, ..policy(1, false) };
        let capital = 1_000 * BILI;
        let mut s = PalwBondBudgetStateV1::new(0, &p);
        let mut j = Vec::new();
        let mut total = 0;
        for span in 0..4u64 {
            let remaining = palw_round_rights_remaining_v1(&p, capital, s.bond_row(&bond(1)));
            let got =
                draw(&finals, 20 + span, 120, &|b| if *b == bond(1) { remaining } else { 0 }).get(&bond(1)).copied().unwrap_or(0);
            s.reserve_round_rights_v1(&p, capital, span, bond(1), 10 + span, got, &mut j).unwrap();
            total += got;
            s.check_consistency().unwrap();
        }
        assert_eq!(total, 30, "four spans inside one window share the bond's 30 rights");
        assert_eq!(palw_round_rights_remaining_v1(&p, capital, s.bond_row(&bond(1))), 0);
        s.release_due(109, &mut j);
        assert_eq!(palw_round_rights_remaining_v1(&p, capital, s.bond_row(&bond(1))), 0, "nothing returns before draw + W");
        s.release_due(113, &mut j);
        assert_eq!(palw_round_rights_remaining_v1(&p, capital, s.bond_row(&bond(1))), 30, "all four draws released by their + W");
        s.check_consistency().unwrap();
    }

    /// **NEUTRALITY**: swapping two bonds' roles — operator id, domain, which is "genesis" (the lower key) and which is new — swaps the
    /// allocation exactly; the input (registration) order is irrelevant.
    #[test]
    fn a_role_swap_swaps_the_allocation_and_order_is_irrelevant() {
        for seed in 0..50u64 {
            let a = exec_final(1, 11, 2_000_000, 1);
            let b = exec_final(2, 12, 2_000_000, 2);
            let one = draw(&[a, b], seed, 25, &|_| 1_000);
            let swapped = |f: PalwExecFinalV1, to: &PalwExecFinalV1| PalwExecFinalV1 {
                bond: to.bond,
                operator_id: to.operator_id,
                domain: to.domain,
                ..f
            };
            let two = draw(&[swapped(a, &b), swapped(b, &a)], seed, 25, &|_| 1_000);
            assert_eq!(one.get(&bond(1)), two.get(&bond(2)), "seed {seed}: the work's allocation follows the work, not the role");
            assert_eq!(one.get(&bond(2)), two.get(&bond(1)));
            assert_eq!(draw(&[b, a], seed, 25, &|_| 1_000), one, "order-free");
        }
    }

    /// **SPLIT**: the same total work and the same total capital split across bonds, operator keys or claims never raise the candidate
    /// total beyond the tolerance (one rounding ticket per Final); at the capital cap a split never raises it at all.
    #[test]
    fn splitting_bonds_or_claims_never_raises_candidates() {
        let p =
            PalwBondBudgetPolicyV1 { round_rights: PalwRoundRightsPolicyV1::ExecutionCap { rights_per_unit: 25 }, ..policy(1, false) };
        let mut rng = Rng(0x5eed_00e3);
        let (mut whole_sum, mut split_sum) = (0u64, 0u64);
        for trial in 0..300u64 {
            let credit = 1 + rng.below(20_000_000);
            let capital = rng.below(10_000 * BILI);
            let k = 1 + rng.below(6);
            // The whole: one bond, one claim.
            let whole_r = palw_round_rights_remaining_v1(&p, capital, None);
            let whole = draw(&[exec_final(1, 1, credit, 1)], trial, 1 << 15, &|_| whole_r).values().sum::<u64>();
            // The split: k bonds (k operator keys), the work in k claims of distinct roots, the capital in k pieces.
            let mut finals = Vec::new();
            let mut pieces = Vec::new();
            let (mut c_left, mut w_left) = (capital, credit);
            for i in 0..k {
                let (c, w) = if i + 1 == k { (c_left, w_left) } else { (rng.below(c_left + 1), rng.below(w_left + 1)) };
                c_left -= c;
                w_left -= w;
                pieces.push((bond(10 + i as u32), palw_round_rights_remaining_v1(&p, c, None)));
                finals.push(exec_final(100 + i, 100 + i, w, 10 + i as u32));
            }
            let caps: BTreeMap<_, _> = pieces.into_iter().collect();
            let split = draw(&finals, trial, 1 << 15, &|b| caps[b]).values().sum::<u64>();
            assert!(split <= whole + k, "trial {trial}: split {split} vs whole {whole} beyond one rounding ticket per Final");
            let r_sum: u64 = caps.values().sum();
            assert!(r_sum <= whole_r, "capital caps are sub-additive");
            if credit / PALW_EXECUTION_QUANTUM_V1 > whole_r + k {
                assert!(split <= whole, "trial {trial}: at the capital cap a split never raises the candidates ({split} > {whole})");
            }
            whole_sum += whole;
            split_sum += split;
        }
        assert!(split_sum <= whole_sum + 300, "in expectation the split is no better ({split_sum} vs {whole_sum})");
    }

    /// **BUDGET**: one row per (span, bond), reserved and used at the draw, a duplicate refused, released only at `draw + W`; in
    /// `CountAgainstBlocks` mode each ticket takes a block of `B`.
    #[test]
    fn round_rights_reserve_once_and_release_at_d_plus_w() {
        let p = policy(1, false); // CountAgainstBlocks; B = 4 blocks per 1,000 BILI
        let capital = 1_000 * BILI;
        let mut s = PalwBondBudgetStateV1::new(0, &p);
        let mut j = Vec::new();
        assert_eq!(palw_round_rights_remaining_v1(&p, capital, None), 4);
        s.reserve_round_rights_v1(&p, capital, 7, bond(1), 50, 3, &mut j).unwrap();
        assert_eq!(s.bond_row(&bond(1)).unwrap().window.block_units, 3 * PALW_BUDGET_BLOCK_UNIT_V1, "fee-only Rounds take B");
        assert_eq!(palw_round_rights_remaining_v1(&p, capital, s.bond_row(&bond(1))), 1);
        assert!(s.clone().reserve_round_rights_v1(&p, capital, 7, bond(1), 51, 1, &mut j).is_err(), "one row per (span, bond)");
        assert!(s.clone().reserve_round_rights_v1(&p, capital, 8, bond(1), 51, 2, &mut j).is_err(), "never past the cap");
        let id = palw_round_rights_row_id_v1(7, &bond(1));
        let row = *s.claim_row(&id).unwrap();
        assert_eq!((row.origin, row.open, row.consumed), (PalwBudgetOriginV1::RoundRights, false, row.reserved));
        s.release_due(149, &mut j);
        assert_eq!(palw_round_rights_remaining_v1(&p, capital, s.bond_row(&bond(1))), 1);
        s.release_due(150, &mut j);
        assert_eq!(palw_round_rights_remaining_v1(&p, capital, s.bond_row(&bond(1))), 4);
        assert!(s.claim_row(&id).is_none(), "the row leaves at its release");
        s.check_consistency().unwrap();
    }

    /// **RFC-0016 P4: a user-paid escrow reward is never clipped by the coinbase-derived model budget.** The same ask on the same
    /// model in the same epoch: an Attempt (coinbase-derived) is clipped to `model_available`; a kernel-route claim (job escrow)
    /// reserves its whole reward, takes nothing from the model's budget and records no model on its row.
    #[test]
    fn rfc0016_p4_user_escrow_is_never_clipped_by_the_model_budget() {
        let p = policy(1, false);
        let ap = PalwModelAllocationPolicyV1 {
            version: PALW_MODEL_ALLOCATION_POLICY_VERSION_V2,
            epoch_daa: 10,
            seasoning_epochs: 1,
            curve: PalwAllocationCurveV1 { alpha_halves: 3 },
            max_models_per_bond: 4,
            average: None,
        };
        let capital = 1_000 * BILI;
        let mut s = PalwBondBudgetStateV1::new(0, &p);
        let mut j = Vec::new();
        s.assign(&ap, bond(1), &[(h(1), 300 * BILI)], 1, capital, 0, |_| true, &mut j).unwrap();
        s.roll_epoch(&ap, 2, 20, |_| (capital, true), &mut j);
        s.accrue(5 * BILI, &mut j);
        assert_eq!(s.model_available(&h(1)), 5 * BILI);
        let ask = PalwBudgetAskV1 { block_units: 0, reward_sompi: 20 * BILI, final_weight: 0, liability_sompi: 0 };
        // Coinbase-derived: clipped to the model's budget (the existing rule).
        let attempt = s.preview_claim_for_origin_v1(&p, capital, bond(1), Some(h(1)), ask, PalwBudgetOriginV1::Attempt).unwrap();
        assert_eq!(attempt.reward_sompi, 5 * BILI);
        // User-paid escrow: the whole escrow, whatever the model budget says.
        let kernel = s.preview_claim_for_origin_v1(&p, capital, bond(1), Some(h(1)), ask, PalwBudgetOriginV1::KernelRoute).unwrap();
        assert_eq!(kernel.reward_sompi, 20 * BILI, "an escrow reward is never clipped by the model allocation");
        let r = s.reserve_claim_v1(&p, capital, h(7), bond(1), Some(h(1)), 21, ask, PalwBudgetOriginV1::KernelRoute, &mut j).unwrap();
        assert_eq!(r.reward_sompi, 20 * BILI);
        assert_eq!(s.claim_row(&h(7)).unwrap().model, None, "an escrow claim records no model-budget reservation");
        assert_eq!(s.model_row(&h(1)).unwrap().reserved_sompi, 0, "the coinbase-derived model budget is untouched");
        assert_eq!(s.model_available(&h(1)), 5 * BILI);
        s.check_consistency().unwrap();
    }

    /// **RFC-0016 P4: an escrow reward the producer's common `R` cap cannot hold is refused BEFORE acceptance** — by the cap
    /// (`Exhausted { Reward }`) or by a per-claim ceiling below the escrow (`EscrowNotReservable`) — and nothing is written.
    #[test]
    fn rfc0016_p4_insufficient_r_refuses_an_escrow_claim_before_acceptance() {
        let capital = 1_000 * BILI;
        // The common cap: R_max = 50 BILI per W at this capital (TEST policy).
        let p = policy(1, false);
        let r_max = palw_bond_budget_caps_v1(&p, capital).reward_sompi;
        let mut s = PalwBondBudgetStateV1::new(0, &p);
        let mut j = Vec::new();
        let ask = |r: u64| PalwBudgetAskV1 { block_units: 0, reward_sompi: r, final_weight: 0, liability_sompi: 0 };
        let before = (s.clone(), j.len());
        assert_eq!(
            s.reserve_claim_v1(&p, capital, h(1), bond(1), None, 5, ask(r_max + 1), PalwBudgetOriginV1::KernelRoute, &mut j),
            Err(PalwBudgetRefusalV1::Exhausted { dim: PalwBudgetDimV1::Reward })
        );
        assert_eq!((s.clone(), j.len()), before, "a refusal writes nothing");
        s.reserve_claim_v1(&p, capital, h(2), bond(1), None, 5, ask(r_max), PalwBudgetOriginV1::KernelRoute, &mut j).unwrap();
        assert_eq!(
            s.preview_claim_for_origin_v1(&p, capital, bond(1), None, ask(1), PalwBudgetOriginV1::KernelRoute),
            Err(PalwBudgetRefusalV1::Exhausted { dim: PalwBudgetDimV1::Reward }),
            "a full R window refuses the next escrow claim instead of reserving less"
        );
        // A per-claim ceiling (rho slicing) below the escrow: refused, where a coinbase-derived ask would be sliced.
        let sliced = policy(3, true);
        let t = PalwBondBudgetStateV1::new(0, &sliced);
        let (r_claim, _) = palw_bond_budget_claim_ceilings_v1(&sliced).unwrap();
        assert_eq!(
            t.preview_claim_for_origin_v1(&sliced, capital, bond(1), None, ask(r_claim + 1), PalwBudgetOriginV1::KernelRoute),
            Err(PalwBudgetRefusalV1::EscrowNotReservable)
        );
        assert_eq!(
            t.preview_claim_for_origin_v1(&sliced, capital, bond(1), None, ask(r_claim + 1), PalwBudgetOriginV1::Attempt)
                .unwrap()
                .reward_sompi,
            r_claim,
            "a coinbase-derived leg keeps the per-claim slice"
        );
        assert!(t.preview_claim_for_origin_v1(&sliced, capital, bond(1), None, ask(r_claim), PalwBudgetOriginV1::KernelRoute).is_ok());
        s.check_consistency().unwrap();
    }

    /// **RFC-0016 / user decision 2026-10-15: switching between Panel=0 (kernel-route, OPV) and Panel=1 (V2 Attempt) claims never
    /// re-acquires issuance capacity.** One bond window holds both modes' reservations, each until its own `accepted + W`: a claim of
    /// the other mode is refused while the first one's reservation is in the window, in both directions, and the window does not reset
    /// at a mode switch. (The per-mode hold — 7-day-equivalent for Panel=0, 1-day-equivalent for Panel=1 — is a code-follow item of
    /// RFC-0016 §11.2; this engine has one `W`, and a per-mode `W` must keep this property.)
    #[test]
    fn rfc0016_a_mode_switch_never_reacquires_issuance_capacity() {
        let p = policy(1, false);
        let capital = 1_000 * BILI;
        let r_max = palw_bond_budget_caps_v1(&p, capital).reward_sompi;
        let ask = |r: u64| PalwBudgetAskV1 { block_units: 0, reward_sompi: r, final_weight: 0, liability_sompi: 0 };
        for (first, second) in [
            (PalwBudgetOriginV1::KernelRoute, PalwBudgetOriginV1::Attempt),
            (PalwBudgetOriginV1::Attempt, PalwBudgetOriginV1::KernelRoute),
        ] {
            let mut s = PalwBondBudgetStateV1::new(0, &p);
            let mut j = Vec::new();
            s.reserve_claim_v1(&p, capital, h(1), bond(1), None, 10, ask(r_max), first, &mut j).unwrap();
            // Terminal early (Final, void or conviction): the issuance window still holds it (ADR-0176 D4).
            s.close(&h(1), &mut j);
            for now in [11, 10 + p.window_daa - 1] {
                s.release_due(now, &mut j);
                assert_eq!(
                    s.preview_claim_for_origin_v1(&p, capital, bond(1), None, ask(1), second),
                    Err(PalwBudgetRefusalV1::Exhausted { dim: PalwBudgetDimV1::Reward }),
                    "{first:?} -> {second:?} at {now}: the other mode does not open a fresh budget"
                );
            }
            s.release_due(10 + p.window_daa, &mut j);
            assert!(s.preview_claim_for_origin_v1(&p, capital, bond(1), None, ask(r_max), second).is_ok(), "free only at d + W");
            s.check_consistency().unwrap();
        }
    }
}

// Read-only native allocation inventory; no state/root/codec field is added.
#[path = "palw_native_resource_v1/private/palw_bond_budget_v1.rs"]
mod native_resource_inventory_v1;
