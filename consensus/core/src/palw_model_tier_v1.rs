//! **ADR-0179 — model tiers: genesis / annual-fork 1.0, on-chain registered 0.3** (user decisions 2026-10-15, rounds 4 and 5;
//! `docs/history/decisions/2026-10/user-decisions-2026-10-15-adr-rfc16-rfc17.md` §7b–§7c). **DORMANT**: the fence `palw_model_tier_v1` is `None` on every
//! preset, hashed Some-only in both fingerprints (its lists and values beside its height), collapsed from `Some(never())`, and refused
//! when armed ([`Params::validate_palw_model_tier_v1`]). Nothing here is built or run by the lane that wrote it; no line is a PASS.
//!
//! ```text
//! ApprovedModels(h)  = the genesis set (classes the genesis assembly registered, and the fence's genesis list)
//!                    ∪ every annual (soft / hard) fork list activated at or below h — "model id + fixed artifact root"
//! ModelTier(m, h)    = 1.0  when (model id, artifact root) ∈ ApprovedModels(h)
//!                    = 0.3  for every other valid on-chain registered model — FIXED from its registration: there is no model review
//!                           at all (round 5), and no elapsed time or registry state (`Active` included) ever makes it 1.0; the only
//!                           route to 1.0 is an annual fork listing
//! c_m                = the tier's coefficient, applied ONCE, in the allocation weight A_m = c_m · S̄_m^1.5 (ADR-0177 V3 roll):
//!                      the resulting P_m feeds RFC-0016 V+ block rights AND the coinbase allocation; an accepted block's reward is
//!                      never multiplied by 0.3 again (never 0.09)
//! registered tier    = Σ over 0.3 models of A_m is capped at κ of Σ A: N registrations never multiply the tier's share; what the
//!                      cap removes is never minted (it is not redistributed). RFC-0016's entrant reserve and RFC-0017's Probation
//!                      budget are MOOT past this fence (no review, no entrants): the reserve is 0 there, κ is the one tier bound
//! review             = ABOLISHED for every model (round 5): past this fence RFC-0017's review (Registered → Probation → Eligible →
//!                      Lapsed, the exam, draws, fees, attempt caps, renewal, lapse) is unreachable
//! per-claim safety   = UNCHANGED for every tier (user decision): Gate C / SecurityCertified, the courts and the drawn-verifier
//!                      certification — an uncertified claim of a genesis or annual model advances nothing. The tier is an economic
//!                      right only
//! ```
//!
//! Annual lists only ADD (ADR-0175): validation refuses a list that names a model id already listed, and membership needs the
//! registered class's own artifact root, so no entry can change or replace an existing model. A listing is a policy decision — never a
//! technical proof that a model is public or independently verifiable.
//!
//! **RFC-0019 (2026-10-15): the storage of `ApprovedModels` is the annual catalog chain** ([`crate::palw_annual_catalog_v1`]) — edition
//! 0 is the genesis catalog, every later edition one annual fork's additions, chained by `previous_catalog_root`. The fence's value
//! carries that chain and nothing parallel to it; the tier API below (`palw_model_tier_v1`, `_at_v1`, `_of_model_v1`,
//! `palw_model_tier_of_v1`, the coefficient, the weight and the cap) is unchanged for its callers.

use crate::Hash64;
use crate::config::params::{ForkActivation, Params};
use crate::palw_annual_catalog_v1::PalwAnnualCatalogChainV1;
use crate::palw_bond_budget_v1::PalwAllocationWeightV1;
use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};
use crate::palw_state_v2::{PalwChainStateV2, PalwStateParamsV2};

/// **The approved tier's coefficient** (1.0).
pub const PALW_MODEL_TIER_APPROVED_PERMILLE_V1: u32 = 1_000;
/// **The registered tier's coefficient** (0.3) — the user's design value; permanent for every on-chain registered model.
pub const PALW_MODEL_TIER_REGISTERED_PERMILLE_DESIGN_V1: u32 = 300;
/// **The registered tier's aggregate cap κ**, permille of the network's total allocation (block rights and coinbase) — a DESIGN
/// value, unapproved: however many models register, the whole 0.3 tier (its shared new-model pool included) never takes more.
pub const PALW_MODEL_TIER_REGISTERED_CAP_PERMILLE_DESIGN_V1: u32 = 300;

/// The fixed-point scale of the registered-tier cap factor.
const PALW_MODEL_TIER_CAP_FACTOR_SCALE_V1: u64 = 1_000_000;

/// **`Params::palw_model_tier_v1`'s value**: the tier system's height, the annual catalog chain (RFC-0019: `ApprovedModels(h)` IS the
/// catalog active at `h` — edition 0 the genesis catalog, each later edition one annual fork's additions) and the two design values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwModelTierFenceV1 {
    pub activation: ForkActivation,
    /// **RFC-0019's catalog chain** — the ONE storage of `ApprovedModels` (no parallel list). Edition 0 names the genesis models
    /// explicitly (classes the genesis assembly registered are approved as a chain fact without being listed); every later edition
    /// activates at its own height, at or after the fence, and only adds.
    pub catalogs: PalwAnnualCatalogChainV1,
    /// The registered tier's coefficient `c`, permille (design 300).
    pub registered_permille: u32,
    /// The registered tier's aggregate cap `κ`, permille (design value).
    pub registered_cap_permille: u32,
}

impl PalwModelTierFenceV1 {
    /// **NOT A PROPOSAL**: the fence at `activation` with the design values and an empty genesis catalog (the fork-id probe, fixtures).
    pub fn at(activation: ForkActivation) -> Self {
        Self {
            activation,
            catalogs: PalwAnnualCatalogChainV1::genesis_only(Vec::new()),
            registered_permille: PALW_MODEL_TIER_REGISTERED_PERMILLE_DESIGN_V1,
            registered_cap_permille: PALW_MODEL_TIER_REGISTERED_CAP_PERMILLE_DESIGN_V1,
        }
    }

    /// **The value's own refusals** — the tiers' shape and RFC-0019's catalog chain ([`PalwAnnualCatalogChainV1::validate`]: edition
    /// 0 at DAA 0, each edition chained to the previous root, strictly increasing real heights, ADD-ONLY — a class id listed once over
    /// the whole chain, so no edition re-lists, replaces or re-roots a model, ADR-0175 — and a kernel version this build implements);
    /// annual editions activate at or after the fence; `0 < c < 1000`, `0 < κ ≤ 1000`.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.registered_permille == 0 || self.registered_permille >= PALW_MODEL_TIER_APPROVED_PERMILLE_V1 {
            return Err("the registered tier's coefficient is strictly between 0 and the approved tier's 1.0");
        }
        if self.registered_cap_permille == 0 || self.registered_cap_permille > 1_000 {
            return Err("the registered tier's aggregate cap is a positive permille, at most 1000");
        }
        self.catalogs.validate()?;
        if self.catalogs.editions.iter().skip(1).any(|c| c.activation_daa < self.activation.daa_score()) {
            return Err("annual catalog editions activate at or after the tier fence");
        }
        Ok(())
    }

    /// The value as the identity hashers write it, after the activation (Some-only): the two values, then the catalog chain (each
    /// edition's number, height and root — the root binds its models and the previous root). `consensus_identity_id`'s normalisation
    /// has already dropped every edition not yet in force ([`PalwAnnualCatalogChainV1::drop_unscheduled_for_identity`]), so a build
    /// carrying the NEXT edition shares the identity of one that does not; the schedule id and the raw params id see every edition.
    pub(crate) fn write_value_into(&self, h: &mut kaspa_hashes::ConsensusParamsId) {
        h.write(self.registered_permille.to_le_bytes());
        h.write(self.registered_cap_permille.to_le_bytes());
        self.catalogs.write_into(h);
    }

    /// The fence's mirror on the V2 state params.
    pub fn mirror(&self) -> PalwModelTierMirrorV1 {
        PalwModelTierMirrorV1 {
            from_daa: self.activation.daa_score(),
            catalogs: self.catalogs.clone(),
            registered_permille: self.registered_permille,
            registered_cap_permille: self.registered_cap_permille,
        }
    }
}

/// **The fence's mirror** (`PalwStateParamsV2::model_tier_v1`, `#[borsh(skip)]`), which the allocation roll, RFC-0017's exemption
/// and the registry RPC read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwModelTierMirrorV1 {
    pub from_daa: u64,
    /// RFC-0019's catalog chain — `ApprovedModels`.
    pub catalogs: PalwAnnualCatalogChainV1,
    pub registered_permille: u32,
    pub registered_cap_permille: u32,
}

impl PalwModelTierMirrorV1 {
    pub fn active_at(&self, daa: u64) -> bool {
        daa >= self.from_daa
    }

    /// Where `(model_id, artifact_root)` is listed at `daa`, if it is: the catalog edition that added it, among the editions active
    /// at `daa` (the root must match: a listing never re-roots a model).
    pub fn listed_at(&self, model_id: &Hash64, artifact_root: &Hash64, daa: u64) -> Option<PalwTierSourceV1> {
        if !self.active_at(daa) {
            return None;
        }
        self.catalogs.listed_at(model_id, artifact_root, daa).map(|catalog| match catalog.edition {
            0 => PalwTierSourceV1::GenesisList,
            edition => PalwTierSourceV1::AnnualFork { edition, from_daa: catalog.activation_daa },
        })
    }
}

/// Why a model is approved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwTierSourceV1 {
    /// A class the genesis assembly registered (`registrant_bond = None`) — a chain fact, no list needed.
    GenesisAssembly,
    /// Named in the genesis catalog (RFC-0019 edition 0).
    GenesisList,
    /// Added by annual catalog edition `edition` (≥ 1), active from `from_daa`.
    AnnualFork { edition: u32, from_daa: u64 },
}

/// **A model's tier.**
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwModelTierV1 {
    /// 1.0 — in `ApprovedModels(h)`.
    Approved(PalwTierSourceV1),
    /// 0.3 — every other valid on-chain registered model, permanently.
    Registered,
}

impl PalwModelTierV1 {
    pub fn is_approved(self) -> bool {
        matches!(self, Self::Approved(_))
    }

    /// The RPC's name: `approved` (1.0) or `registered` (0.3).
    pub fn name(self) -> &'static str {
        match self {
            Self::Approved(_) => "approved",
            Self::Registered => "registered",
        }
    }
}

/// **The tier of `(model_id, artifact_root)` at `daa`** (pure): a genesis-assembly class, or a listed `(id, root)`, is approved; every
/// other model is registered. Only `ApprovedModels` decides — no review outcome, elapsed time or registry state is an input.
pub fn palw_model_tier_of_v1(
    mirror: Option<&PalwModelTierMirrorV1>,
    model_id: &Hash64,
    artifact_root: &Hash64,
    genesis_assembly: bool,
    daa: u64,
) -> PalwModelTierV1 {
    if genesis_assembly {
        return PalwModelTierV1::Approved(PalwTierSourceV1::GenesisAssembly);
    }
    match mirror.and_then(|m| m.listed_at(model_id, artifact_root, daa)) {
        Some(source) => PalwModelTierV1::Approved(source),
        None => PalwModelTierV1::Registered,
    }
}

/// **The allocation coefficient `c_m`**, permille: 1000 below the fence (every model weighs as before, A-2) and for the approved tier;
/// the registered coefficient past it.
pub fn palw_model_tier_coefficient_permille_v1(mirror: Option<&PalwModelTierMirrorV1>, tier: PalwModelTierV1, daa: u64) -> u32 {
    match (mirror.filter(|m| m.active_at(daa)), tier) {
        (Some(m), PalwModelTierV1::Registered) => m.registered_permille,
        _ => PALW_MODEL_TIER_APPROVED_PERMILLE_V1,
    }
}

/// **`c_m · S̄_m^α`** — the ONE place the coefficient is applied: `⌊weight · c / 1000⌋`.
pub fn palw_model_tier_weight_v1(weight: PalwAllocationWeightV1, c_permille: u32) -> PalwAllocationWeightV1 {
    if c_permille >= PALW_MODEL_TIER_APPROVED_PERMILLE_V1 {
        return weight;
    }
    let (product, overflow) = weight.overflowing_mul_u64(c_permille as u64);
    if overflow {
        // Unreachable under PALW_ALLOCATION_ALPHA_HALVES_MAX_V1 (A_m < 2^256); allocate nothing rather than guess.
        return PalwAllocationWeightV1::ZERO;
    }
    product.div_rem_u64(PALW_MODEL_TIER_APPROVED_PERMILLE_V1 as u64).0
}

/// **The registered tier's cap factor** in ppm (ADR-0179 5 with the 2026-10-22 user correction: `ΣA` is the sum of the SAME scaled
/// `A_m` the rows carry). `canonical` is the approved tier's `Σ A_m`, `registered` the registered tier's `Σ c_m·S̄_m^α` before the
/// cap. 1,000,000 while `registered ≤ κ/(1−κ) · canonical` (the registered tier is at most `κ` of the scaled `ΣA`); past it
/// `⌊κ · canonical · 10^6 / ((1000 − κ) · registered)⌋`, so the scaled registered total is at most `κ` of the scaled denominator and
/// capital on registered models never dilutes the approved tier beyond the registered tier's scaled weight. `κ = 1000` never binds.
/// With no approved weight at all the registered tier is at most `κ` of nothing: factor 0.
pub fn palw_model_tier_cap_factor_ppm_v1(
    canonical: PalwAllocationWeightV1,
    registered: PalwAllocationWeightV1,
    cap_permille: u32,
) -> u64 {
    if registered.is_zero() || cap_permille >= PALW_MODEL_TIER_APPROVED_PERMILLE_V1 {
        return PALW_MODEL_TIER_CAP_FACTOR_SCALE_V1;
    }
    let rest = (PALW_MODEL_TIER_APPROVED_PERMILLE_V1 - cap_permille) as u64;
    let (lhs, o1) = registered.overflowing_mul_u64(rest);
    let (rhs, o2) = canonical.overflowing_mul_u64(cap_permille as u64);
    if o1 || o2 {
        return 0;
    }
    if lhs <= rhs {
        return PALW_MODEL_TIER_CAP_FACTOR_SCALE_V1;
    }
    // κ · canonical · 10^6 / ((1000 − κ) · registered)  (< 10^6 here).
    let (num, o3) = rhs.overflowing_mul_u64(PALW_MODEL_TIER_CAP_FACTOR_SCALE_V1);
    if o3 {
        return 0;
    }
    let (q, _) = num.div_rem(lhs);
    if q > PalwAllocationWeightV1::from_u64(PALW_MODEL_TIER_CAP_FACTOR_SCALE_V1) {
        PALW_MODEL_TIER_CAP_FACTOR_SCALE_V1
    } else {
        q.as_u64()
    }
}

/// `⌊weight · factor_ppm / 10^6⌋`.
pub fn palw_model_tier_scale_v1(weight: PalwAllocationWeightV1, factor_ppm: u64) -> PalwAllocationWeightV1 {
    if factor_ppm >= PALW_MODEL_TIER_CAP_FACTOR_SCALE_V1 {
        return weight;
    }
    let (product, overflow) = weight.overflowing_mul_u64(factor_ppm);
    if overflow {
        return PalwAllocationWeightV1::ZERO;
    }
    product.div_rem_u64(PALW_MODEL_TIER_CAP_FACTOR_SCALE_V1).0
}

/// **The narrow interface: a class's tier at `daa`** (the class's own id and artifact root; a class the chain does not have is
/// `Registered`, the conservative side). RFC02's registration lanes and drawn-verifier approval consume this.
pub fn palw_model_tier_at_v1(state: &PalwChainStateV2, params: &PalwStateParamsV2, class_id: &Hash64, daa: u64) -> PalwModelTierV1 {
    match state.class(class_id) {
        Some(class) => {
            let mirror = params.model_tier_v1();
            let tier = palw_model_tier_of_v1(mirror, class_id, &class.artifact_root, class.registrant_bond.is_none(), daa);
            if tier.is_approved() {
                return tier;
            }
            // **RFC-0002 Tensor IR identity (lane rfc02-09)**: a genesis-8 catalog names a TIR model by its `PALWART3` container
            // root, while the class registers (and its courts open) the inner inventory root. The bound container root — recorded
            // only past `palw_tir_container_identity_v1`, read only at or after the DAA it was bound — is the catalog key the
            // edition was written against. Below that fence no row exists and this is byte-identical to the registered-root read.
            match crate::palw_artifact_identity_v1::palw_catalog_container_root_of_class_v1(state, class_id, daa) {
                Some(container) => palw_model_tier_of_v1(mirror, class_id, &container.hash(), false, daa),
                None => tier,
            }
        }
        None => PalwModelTierV1::Registered,
    }
}

/// **`palw_model_tier_v1(state, params, class) -> tier`** — [`palw_model_tier_at_v1`] at the state's own tip DAA (its last folded
/// point; 0 for a state with none).
pub fn palw_model_tier_v1(state: &PalwChainStateV2, params: &PalwStateParamsV2, class_id: &Hash64) -> PalwModelTierV1 {
    let daa = state.last_point().map_or(0, |point| point.daa_score);
    palw_model_tier_at_v1(state, params, class_id, daa)
}

/// **The tier of an ALLOCATION model key** — a class (its founding line), or a model line keyed by its own id.
///
/// audit(compute-weight) W-1 (§9.1–9.2, ADR-0175): a tier belongs to `(model id, artifact root)`, never to the graph a model runs.
/// * **A non-founding line is a later registration** — its own id, its own root (other weights: `claim_artifact_root` refuses the
///   class's own root to any other line) — so it is `Registered` (0.3), whatever its class's tier. It used to be read through
///   `line.class_id`, i.e. the CLASS's root and registrant, so any weights on an approved graph — an all-zero artifact whose trace costs
///   nothing — collected 1.0 outside the registered tier's cap κ.
/// * **A class keeps an approved tier only while every root in force on its founding line is the class's own** (the root a listing
///   names, or the one the genesis assembly registered). A claim naming a later founding-line version is keyed to the class id
///   (`artifact_line_of_root`), so an approved id running unlisted weights is `Registered` until those weights leave force.
///
/// Read only by the tiered allocation roll, i.e. past `palw_model_tier_v1` (dormant on every preset): below it nothing changes.
pub fn palw_model_tier_of_model_v1(state: &PalwChainStateV2, params: &PalwStateParamsV2, model: &Hash64, daa: u64) -> PalwModelTierV1 {
    let Some(class) = state.class(model) else {
        // A model line that is not a class's founding line, or an unknown key: never in `ApprovedModels`.
        return PalwModelTierV1::Registered;
    };
    let tier = palw_model_tier_at_v1(state, params, model, daa);
    if tier.is_approved() && palw_founding_line_runs_another_root_v1(state, model, &class.artifact_root, daa) {
        return PalwModelTierV1::Registered;
    }
    tier
}

/// Whether the class's founding line (its id is the class id) has a version in force at `daa` whose root is not `class_root`. A
/// founding line nothing touched is the class's registration (version 1 = `class_root`) and answers `false`.
fn palw_founding_line_runs_another_root_v1(state: &PalwChainStateV2, class_id: &Hash64, class_root: &Hash64, daa: u64) -> bool {
    let Some(line) = state.model_line_or_founding(class_id) else { return false };
    if line.retired_daa.is_some_and(|retired| daa >= retired) {
        return false;
    }
    (1..=line.versions_published).any(|n| state.model_version(class_id, n).is_some_and(|v| v.in_force_at(daa) && v.root != *class_root))
}

impl PalwStateParamsV2 {
    /// Written by `Params::sync_palw_model_tier_v1` (and fixtures) only.
    pub fn with_model_tier_v1(mut self, mirror: Option<PalwModelTierMirrorV1>) -> Self {
        self.model_tier_v1 = mirror;
        self
    }

    /// The tier fence's mirror, if the network arms it.
    pub fn model_tier_v1(&self) -> Option<&PalwModelTierMirrorV1> {
        self.model_tier_v1.as_ref()
    }
}

impl Params {
    /// `palw_model_tier_v1`'s height, `Some` only for a real one.
    pub fn palw_model_tier_v1_from_daa(&self) -> Option<u64> {
        self.palw_model_tier_v1.as_ref().filter(|f| f.activation != ForkActivation::never()).map(|f| f.activation.daa_score())
    }

    fn palw_model_tier_v1_mirror(&self) -> Option<PalwModelTierMirrorV1> {
        self.palw_model_tier_v1.as_ref().filter(|f| f.activation != ForkActivation::never()).map(|f| f.mirror())
    }

    /// **The fence's mirror** on the V2 bundle's state params; `None` where it is not armed.
    pub fn sync_palw_model_tier_v1(&mut self) {
        let mirror = self.palw_model_tier_v1_mirror();
        if let PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_model_tier_v1(mirror);
        }
    }

    /// **The fence's refusals**: the mirror agrees; a malformed value (the add-only rule included) is named; the allocation it is
    /// applied in (`palw_model_bond_allocation_v1`) and RFC-0018 Gate C (`palw_independently_work_certified_v1` — with model review
    /// abolished, the per-claim drawn certification is the one safety gate, for every tier) at or below it; then the arm-readiness
    /// record for exactly this network and height, naming the fence's own c, κ and catalog tip ([`PalwModelTierArmingRecordV1`]).
    /// [`Self::validate_palw_model_tier_v1`] with `records` as the pinned list (tests and arm-readiness tooling only).
    pub fn validate_palw_model_tier_with_records_v1(&self, records: &[PalwModelTierArmingRecordV1]) -> Result<(), PalwModeV2Error> {
        if let PalwConsensusMode::ConsensusV2(bundle) = &self.palw_consensus_mode
            && bundle.state.model_tier_v1().cloned() != self.palw_model_tier_v1_mirror()
        {
            return Err(PalwModeV2Error::Invalid(
                "palw_model_tier_v1 disagrees with the V2 bundle's mirror: Params::sync_palw_model_tier_v1",
            ));
        }
        let Some(fence) = self.palw_model_tier_v1.as_ref().filter(|f| f.activation != ForkActivation::never()) else { return Ok(()) };
        fence.validate().map_err(PalwModeV2Error::Invalid)?;
        let at = fence.activation.daa_score();
        let below = |f: Option<ForkActivation>| f.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= at);
        if !below(self.palw_model_bond_allocation_v1.as_ref().map(|f| f.activation)) {
            return Err(PalwModeV2Error::Invalid(
                "palw_model_tier_v1 needs palw_model_bond_allocation_v1 at or below it: c_m is applied once, in the allocation weight",
            ));
        }
        if !below(self.palw_independently_work_certified_v1) {
            return Err(PalwModeV2Error::Invalid(
                "palw_model_tier_v1 needs palw_independently_work_certified_v1 (RFC-0018 Gate C) at or below it: with no model review, per-claim certification is the safety gate for every tier",
            ));
        }
        // RFC-0019 §16 / ADR-0179: the catalog chain is the value; c, κ and every edition's release process (signed manifest,
        // mixed-binary drills across the switch) are admitted only by a pinned arm-readiness record (lane rfcgap-b, 2026-10-22: was a
        // fixed refusal). The list is empty, so arming stays refused whatever the chain says.
        palw_model_tier_arming_record_admits_v1(self.genesis.hash, fence, records).map_err(PalwModeV2Error::Invalid)
    }

    /// **The fence's refusals with the binary's pinned record list** ([`PALW_MODEL_TIER_ARMING_RECORDS_V1`], empty).
    pub fn validate_palw_model_tier_v1(&self) -> Result<(), PalwModeV2Error> {
        self.validate_palw_model_tier_with_records_v1(PALW_MODEL_TIER_ARMING_RECORDS_V1)
    }
}

// ---- The arm-readiness record gate (ADR-0179 数値表, RFC-0019 §16; lane rfcgap-b 2026-10-22) -------------------------------------

/// The evidence a tier arm-readiness record must carry, one digest per entry, in this order (ADR-0179's gate and RFC-0019 §16's
/// `ACTIVATION_ALLOWED`). The values themselves are not evidence: the record names them and must equal the fence's.
pub const PALW_MODEL_TIER_ARMING_REQUIREMENTS_V1: &[&str] = &[
    "RFC-0019 U-6 / ADR-0179: the user's approval of the registered coefficient c and the registered-tier cap κ named by this record, with their measured record",
    "ADR-0179 test sources PASS (palw_model_tier_v1 tests and the tiered allocation roll across reorg / IBD / restart)",
    "RFC-0019 §11 unit tests A–E PASS",
    "RFC-0019 R-1–R-3, R-5, R-6 run with the shipped binary, convergence recorded",
    "RFC-0019 R-4 boundary for every edition that lists a model at 1.0 after the fence (or the record states none does)",
    "RFC-0019 R-7 model-add and R-8 local-llm for every edition adding a new local LLM (or the record states none does)",
    "every catalog edition's signed release manifest verifies (misaka/release-manifest/v1: signatures, checksums, reproduction, fingerprint, catalog, add-only)",
    "RFC-0019 readiness M1–M5 reported to the user",
];

/// The record format's version.
pub const PALW_MODEL_TIER_ARMING_RECORD_VERSION_V1: u16 = 1;

/// **A tier arm-readiness record** for one network and fence height. It names the approved `c` and `κ` and the catalog chain's tip
/// (last edition number and root), which must equal the fence's value: a new annual edition changes the tip root, so every edition
/// needs its own record (RFC-0019 §16: "palw_model_tier_v1 とその各 edition").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwModelTierArmingRecordV1 {
    pub version: u16,
    pub genesis: Hash64,
    pub activation_daa: u64,
    pub registered_permille: u32,
    pub registered_cap_permille: u32,
    pub catalog_tip_edition: u32,
    pub catalog_tip_root: Hash64,
    pub source_commit: [u8; 20],
    pub evidence: &'static [[u8; 32]],
    pub review: [u8; 32],
}

/// **The pinned records — EMPTY.** No `κ` is approved and no edition has a release record: every armed height is refused.
// Lane ACT2 (2026-10-27): testnet-12 DAA 17,000 only (c 300, κ 1000 = user decision), every slot a typed waiver.
pub const PALW_MODEL_TIER_ARMING_RECORDS_V1: &[PalwModelTierArmingRecordV1] =
    crate::palw_t12_17000_records_v1::PALW_T12_17000_MODEL_TIER_RECORDS_V1;

/// Does `records` authorize `fence` on the network `genesis`? The first record naming exactly this (network, height) decides; it must
/// name the fence's own `c`, `κ` and catalog tip, and its form must hold. `Err` carries the refusal text.
pub fn palw_model_tier_arming_record_admits_v1(
    genesis: Hash64,
    fence: &PalwModelTierFenceV1,
    records: &[PalwModelTierArmingRecordV1],
) -> Result<(), &'static str> {
    let at = fence.activation.daa_score();
    let Some(r) = records.iter().find(|r| r.genesis == genesis && r.activation_daa == at) else {
        return Err(
            "palw_model_tier_v1 cannot be armed: no arm-readiness record is pinned for this network and height \
             (PALW_MODEL_TIER_ARMING_RECORDS_V1 is empty: c and κ are design values without approval and a measured record, and no annual \
             catalog edition has a signed release manifest and recorded mixed-binary drills; ADR-0179, RFC-0019 §16)",
        );
    };
    if r.version != PALW_MODEL_TIER_ARMING_RECORD_VERSION_V1 {
        return Err("the model-tier arm-readiness record's version is not this binary's");
    }
    if r.registered_permille != fence.registered_permille || r.registered_cap_permille != fence.registered_cap_permille {
        return Err("the model-tier arm-readiness record approves another c or κ than the fence carries");
    }
    let Some(tip) = fence.catalogs.editions.last() else {
        return Err("the model-tier fence carries no catalog edition");
    };
    if r.catalog_tip_edition != tip.edition || r.catalog_tip_root != tip.root() {
        return Err("the model-tier arm-readiness record was made for another catalog chain (each edition needs its own record)");
    }
    if r.source_commit == [0; 20] {
        return Err("the model-tier arm-readiness record names no source commit");
    }
    if r.evidence.len() != PALW_MODEL_TIER_ARMING_REQUIREMENTS_V1.len() {
        return Err("the model-tier arm-readiness record does not carry one evidence digest per requirement");
    }
    if r.review == [0; 32] {
        return Err("the model-tier arm-readiness record carries no review digest");
    }
    for (i, d) in r.evidence.iter().enumerate() {
        if *d == [0; 32] || *d == r.review || r.evidence[..i].contains(d) {
            return Err("a model-tier arm-readiness evidence digest is zero, repeated, or equal to the review");
        }
    }
    // Lane ACT2: a slot that is a pinned waiver must be that waiver's own slot (`crate::palw_arming_waiver_v1`).
    crate::palw_arming_waiver_v1::palw_arming_record_waivers_hold_v1(
        "palw_model_tier_v1",
        r.genesis,
        r.activation_daa,
        PALW_MODEL_TIER_ARMING_REQUIREMENTS_V1,
        r.evidence,
        &r.review,
        crate::palw_arming_waiver_v1::PALW_ARMING_WAIVERS_V1,
    )
    .map_err(|(_, e)| e)?;
    Ok(())
}

#[cfg(test)]
mod tests;
