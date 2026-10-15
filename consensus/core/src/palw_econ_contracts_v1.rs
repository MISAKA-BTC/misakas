//! **RFC07 R7-ECON / RFC08 R8-WINDOW / R8-ECON: three dormant signed economic contracts** (2026-10-15; design audit requirement
//! index R7-ECON, R8-WINDOW, R8-ECON; RFC-0007 and RFC-0008 2026-10-15 spec-closure sections; ADR-0177 D1; user rulings 2026-10-10).
//!
//! | fence (dormant, `None` on every preset) | kernel inner | kernel route aux | what |
//! |---|---|---|---|
//! | `palw_model_verification_gate_v1` (needs `palw_model_bond_allocation_v1` at or below) | 67 | 125 | the optional verification-attestation gate on the model allocation |
//! | `palw_horizon_extension_v1` (needs `palw_exec_payload_v2` at or below) | 68 | 126 | consent-signed extension of a live EXEC dependency horizon |
//! | `palw_minimum_price_v1` (needs `palw_probabilistic_constraints_v1` at or below) | 69 | 127 | the worker/poster minimum-price contract |
//!
//! **The gate (R7-ECON lazy verifier / ADR-0177 D1).** Per model and allocation epoch the consumer opens a tally from a verified beacon
//! output: `m` slots drawn by stake (with replacement) from the global verifier pool, and one beacon-chosen Final claim of the model.
//! Each slot holder attests (inner 67) "verified" or "could not verify" for that exact claim. The epoch's allocation subsidy is payable
//! only when at least `k` slots attest "verified"; otherwise it is withheld (never minted). The ONLY withholding reason is unattested
//! verification — a model without a Final claim to sample is never withheld by the gate ("not served" is not a reason). Claims, user
//! escrow, Final, weight and slashing are untouched. M\*-49's per-check fee is the kernel's (`misaka_palw_kernel::verifier_pay`; its
//! attestation is inner 39 under `palw_verifier_pay_v1`).
//!
//! **The horizon extension (R8-WINDOW item 3 / 9).** Every affected party — the worker (the claim's producer) and the sponsor (its job's
//! poster) — signs the same terms; a transferred claim's providers consent through their own signed leases, which must already cover
//! the new horizon (§15.5: leases are never rewritten or lengthened here). The extension applies once every party signed: it only
//! lengthens (never shortens a Final or court window), never passes the claim-fixed hard deadline (X3), never reopens an expired
//! horizon, never cancels earned cash, and extends the EXEC income hold with the same horizon. Journaled through the route's aux and
//! ledger rows; restore re-checks every applied row against the ledger.
//!
//! **The minimum-price contract (R8-ECON, §15.7).** `p_collect_lower · M_net_min ≥ C_upper`, with `C_upper` = the class's measured
//! upper cost + `capital_rate` × the capital-time area counted once. Both parties sign the same terms before the worker is reserved; the
//! contract binds the funded amount (the job's GAP-5 escrow must cover it), scope (job/class), worker, policy, deadline and retention.
//! An offer below the derived minimum, an unmeasured class, a zero collection floor (UNKNOWN) or an unrepresentable quote is refused;
//! while a contract governs a job, only its worker's claim is admitted, and only inside its acceptance deadline.
//!
//! **No value here is approved.** Every parameter is a named field of a fenced policy; `unapproved_probe_*` exist only for tests and
//! private profiles, and `validate_*` refuses every real activation like the sibling fences.

use crate::config::params::{ForkActivation, Params};
use crate::palw_mode_v2::PalwModeV2Error;
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::Hash64;
pub use misaka_palw_kernel::econ_contracts_v1::{
    HorizonExtensionTermsV1, MinimumPriceContractTermsV1, ModelVerificationAttestationV1, ModelVerificationVerdictV1,
};
use misaka_palw_kernel::hash::object_id;

type Digest = [u8; 64];

/// Kernel route aux table: model-epoch verification tallies (key: tally id, 64 bytes).
pub const PALW_MODEL_VERIFICATION_TABLE_V1: u8 = 125;
/// Kernel route aux table: consented horizon extensions (key: `0x01 ‖ terms id`).
pub const PALW_HORIZON_EXTENSION_TABLE_V1: u8 = 126;
/// Kernel route aux table: minimum-price contracts (`0x01 ‖ contract id`) and their job index (`0x02 ‖ job`).
pub const PALW_MINIMUM_PRICE_TABLE_V1: u8 = 127;
pub const HORIZON_EXTENSION_PREFIX_V1: u8 = 0x01;
pub const MINIMUM_PRICE_CONTRACT_PREFIX_V1: u8 = 0x01;
pub const MINIMUM_PRICE_JOB_PREFIX_V1: u8 = 0x02;

/// The user's INTERIM, unapproved gate size (ruling 2026-10-10, ECON §5c.4 at σ = 1/3): `m` = 47 slots, `k` = 24 attestations. Probe
/// values only — not an activation value, and the fence stays refused.
pub const PALW_MODEL_VERIFICATION_SLOTS_INTERIM_V1: u16 = 47;
pub const PALW_MODEL_VERIFICATION_THRESHOLD_INTERIM_V1: u16 = 24;
/// The structural bound on `m` (a tally row stays a bounded record).
pub const PALW_MODEL_VERIFICATION_SLOTS_MAX_V1: u16 = 255;

fn prefixed(prefix: u8, id: &Digest) -> Vec<u8> {
    let mut key = Vec::with_capacity(65);
    key.push(prefix);
    key.extend_from_slice(id);
    key
}

// ---- A. the verification-attestation gate ----------------------------------------------------------------------------------------

/// How the tally chooses the claim every slot checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwVerificationSampleRuleV1 {
    /// One Final claim of the model-epoch, uniform over the canonical (sorted) set, indexed by the beacon output.
    UniformFinalClaimV1 = 0,
}

/// **The gate's policy** — every value POLICY, none approved.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwModelVerificationGatePolicyV1 {
    pub version: u16,
    /// `m`: slots drawn by stake per model-epoch.
    pub slots: u16,
    /// `k`: "verified" slots the allocation needs.
    pub threshold: u16,
    pub sample_rule: PalwVerificationSampleRuleV1,
    /// From a tally's opening to its last counted attestation (must hold beacon + fetch + check + localisation + filing + margin).
    pub attestation_window_daa: u64,
    /// Undecided tallies held at once.
    pub maximum_open_tallies: u32,
    pub security_review_scope: Digest,
    pub economic_review_scope: Digest,
}

impl PalwModelVerificationGatePolicyV1 {
    pub fn id(&self) -> Hash64 {
        Hash64::from_bytes(object_id(b"MISAKA/PALW/MODEL-VERIFICATION-GATE-POLICY/V1", self))
    }

    /// Structural invariants only: `0 < k ≤ m ≤ 255` and `2k > m` (two disjoint slot sets can never both reach `k`), a finite non-zero
    /// window and capacity, and explicit review identities. Sizing `(m, k)` for an adversary share is POLICY (ECON §5c.4).
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.version != 1
            || self.slots == 0
            || self.slots > PALW_MODEL_VERIFICATION_SLOTS_MAX_V1
            || self.threshold == 0
            || self.threshold > self.slots
            || u32::from(self.threshold) * 2 <= u32::from(self.slots)
            || self.attestation_window_daa == 0
            || self.attestation_window_daa == u64::MAX
            || self.maximum_open_tallies == 0
            || self.security_review_scope == [0; 64]
            || self.economic_review_scope == [0; 64]
        {
            return Err("the verification gate needs 0 < k <= m <= 255 with 2k > m, a finite window and capacity and review scopes");
        }
        Ok(())
    }

    /// **NOT A PROPOSAL**: the user's interim `m`/`k` with structural fixture bounds, for tests and private profiles only.
    pub fn unapproved_probe_v1() -> Self {
        Self {
            version: 1,
            slots: PALW_MODEL_VERIFICATION_SLOTS_INTERIM_V1,
            threshold: PALW_MODEL_VERIFICATION_THRESHOLD_INTERIM_V1,
            sample_rule: PalwVerificationSampleRuleV1::UniformFinalClaimV1,
            attestation_window_daa: 110,
            maximum_open_tallies: 64,
            security_review_scope: [1; 64],
            economic_review_scope: [2; 64],
        }
    }
}

/// `Params::palw_model_verification_gate_v1`'s value — Some-only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwModelVerificationGateFenceV1 {
    pub activation: ForkActivation,
    pub policy: PalwModelVerificationGatePolicyV1,
}

impl PalwModelVerificationGateFenceV1 {
    pub fn unapproved_probe_at(activation: ForkActivation) -> Self {
        Self { activation, policy: PalwModelVerificationGatePolicyV1::unapproved_probe_v1() }
    }
}

/// The tally id of `(model, epoch)`.
pub fn palw_model_verification_tally_id_v1(model: &Digest, epoch: u64) -> Digest {
    object_id(b"MISAKA/PALW/MODEL-VERIFICATION-TALLY/V1", &(*model, epoch))
}

fn beacon_index_v1(domain: &[u8], beacon: &Digest, tally: &Digest, i: u32) -> u128 {
    let h = object_id(domain, &(*beacon, *tally, i));
    u128::from_le_bytes(h[..16].try_into().expect("16 bytes"))
}

/// **The draw**: `slots` holders by stake, with replacement (Sybil-neutral: a split stake draws as the whole). `pool` must be canonical
/// — strictly ascending bond ids, every stake positive — or it is refused (the consumer derives it; it is never caller-ordered).
pub fn palw_model_verification_draw_v1(
    beacon: &Digest,
    tally: &Digest,
    pool: &[(Digest, u64)],
    slots: u16,
) -> Result<Vec<Digest>, &'static str> {
    if pool.is_empty() || slots == 0 {
        return Err("the verification draw needs a non-empty pool and at least one slot");
    }
    let mut total = 0u128;
    for (i, (bond, stake)) in pool.iter().enumerate() {
        if *stake == 0 || (i > 0 && pool[i - 1].0 >= *bond) {
            return Err("the verifier pool is not canonical (ascending distinct bonds, positive stakes)");
        }
        total += u128::from(*stake);
    }
    let mut out = Vec::with_capacity(usize::from(slots));
    for i in 0..u32::from(slots) {
        // `total < 2^80` for any real pool, so the modulo bias against a 128-bit draw is below 2^-48 (a review input, not a claim).
        let mut point = beacon_index_v1(b"MISAKA/PALW/MODEL-VERIFICATION-DRAW/V1", beacon, tally, i) % total;
        let mut chosen = pool[pool.len() - 1].0;
        for (bond, stake) in pool {
            if point < u128::from(*stake) {
                chosen = *bond;
                break;
            }
            point -= u128::from(*stake);
        }
        out.push(chosen);
    }
    Ok(out)
}

/// **The sample**: one Final claim of the model-epoch (`None`: there is none — the gate then withholds nothing). `final_claims` must be
/// canonical (strictly ascending).
pub fn palw_model_verification_sample_v1(
    rule: PalwVerificationSampleRuleV1,
    beacon: &Digest,
    tally: &Digest,
    final_claims: &[Digest],
) -> Result<Option<Digest>, &'static str> {
    if final_claims.windows(2).any(|w| w[0] >= w[1]) {
        return Err("the sampled Final claim set is not canonical");
    }
    if final_claims.is_empty() {
        return Ok(None);
    }
    match rule {
        PalwVerificationSampleRuleV1::UniformFinalClaimV1 => {
            let i = beacon_index_v1(b"MISAKA/PALW/MODEL-VERIFICATION-SAMPLE/V1", beacon, tally, 0) % final_claims.len() as u128;
            Ok(Some(final_claims[i as usize]))
        }
    }
}

/// A model-epoch tally (aux 125).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwModelVerificationTallyV1 {
    pub model: Digest,
    pub epoch: u64,
    pub beacon: Digest,
    pub model_root: Digest,
    pub sampled_claim: Option<Digest>,
    /// Drawn holders in draw order, each with its verdict (`Unset` until it attests).
    pub slots: Vec<(Digest, ModelVerificationVerdictV1)>,
    pub opened_daa: u64,
    pub deadline_daa: u64,
    pub policy: Hash64,
}

/// What the gate says about a model-epoch's allocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwModelVerificationOutcomeV1 {
    Pending = 0,
    Attested = 1,
    /// Verification was not attested by `k` slots by the deadline (or can no longer be): the subsidy is withheld, not minted.
    Unattested = 2,
    /// No Final claim existed to sample: the gate withholds nothing (D1: never "the model was not served").
    NoFinalClaim = 3,
}

impl PalwModelVerificationTallyV1 {
    pub fn verified_slots(&self) -> usize {
        self.slots.iter().filter(|(_, v)| *v == ModelVerificationVerdictV1::Verified).count()
    }

    pub fn outcome(&self, threshold: u16, now: u64) -> PalwModelVerificationOutcomeV1 {
        if self.sampled_claim.is_none() {
            return PalwModelVerificationOutcomeV1::NoFinalClaim;
        }
        let verified = self.verified_slots();
        if verified >= usize::from(threshold) {
            return PalwModelVerificationOutcomeV1::Attested;
        }
        let open = self.slots.iter().filter(|(_, v)| *v == ModelVerificationVerdictV1::Unset).count();
        if now > self.deadline_daa || verified + open < usize::from(threshold) {
            return PalwModelVerificationOutcomeV1::Unattested;
        }
        PalwModelVerificationOutcomeV1::Pending
    }

    pub fn validate(&self, policy: &PalwModelVerificationGatePolicyV1) -> Result<(), &'static str> {
        if self.slots.len() != usize::from(policy.slots)
            || self.policy != policy.id()
            || self.deadline_daa < self.opened_daa
            || self.deadline_daa - self.opened_daa != policy.attestation_window_daa
            || [self.model, self.beacon, self.model_root].contains(&[0; 64])
            || (self.sampled_claim.is_none() && self.slots.iter().any(|(_, v)| *v != ModelVerificationVerdictV1::Unset))
        {
            return Err("a verification tally differs from its policy");
        }
        Ok(())
    }
}

/// The gate's effect on one allocation amount.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwGatedSubsidyV1 {
    pub payable: u64,
    /// Withheld for unattested verification — never minted (P-7).
    pub withheld: u64,
    /// Held until the tally decides.
    pub deferred: u64,
}

/// **The subsidy rule.** Only the allocation subsidy is gated; nothing else reads the outcome.
pub fn palw_model_verification_subsidy_v1(outcome: PalwModelVerificationOutcomeV1, allocation_sompi: u64) -> PalwGatedSubsidyV1 {
    match outcome {
        PalwModelVerificationOutcomeV1::Attested | PalwModelVerificationOutcomeV1::NoFinalClaim => {
            PalwGatedSubsidyV1 { payable: allocation_sompi, ..Default::default() }
        }
        PalwModelVerificationOutcomeV1::Unattested => PalwGatedSubsidyV1 { withheld: allocation_sompi, ..Default::default() },
        PalwModelVerificationOutcomeV1::Pending => PalwGatedSubsidyV1 { deferred: allocation_sompi, ..Default::default() },
    }
}

// ---- B. the consented horizon extension ------------------------------------------------------------------------------------------

/// **The extension's policy** — every value POLICY, none approved.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwHorizonExtensionPolicyV1 {
    pub version: u16,
    /// The largest lengthening one extension grants.
    pub maximum_extension_daa: u64,
    /// The largest span from a consent's landing to its `consent_by_daa`.
    pub maximum_consent_window_daa: u64,
    /// X3 (local): the claim-fixed absolute bound counted from the claim's acceptance `d0`.
    pub maximum_claim_horizon_daa: u64,
    /// Unapplied extensions held at once.
    pub maximum_open_extensions: u32,
    /// Whether the longer retention must be priced by a signed minimum-price contract (inner 69).
    pub require_price_contract: bool,
    pub security_review_scope: Digest,
    pub economic_review_scope: Digest,
}

impl PalwHorizonExtensionPolicyV1 {
    pub fn id(&self) -> Hash64 {
        Hash64::from_bytes(object_id(b"MISAKA/PALW/HORIZON-EXTENSION-POLICY/V1", self))
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.version != 1
            || self.maximum_extension_daa == 0
            || self.maximum_consent_window_daa == 0
            || self.maximum_claim_horizon_daa == 0
            || self.maximum_claim_horizon_daa == u64::MAX
            || self.maximum_extension_daa > self.maximum_claim_horizon_daa
            || self.maximum_open_extensions == 0
            || self.security_review_scope == [0; 64]
            || self.economic_review_scope == [0; 64]
        {
            return Err("the horizon extension needs finite non-zero bounds and review scopes");
        }
        Ok(())
    }

    /// **NOT A PROPOSAL**: structural fixture values for tests and private profiles.
    pub fn unapproved_probe_v1() -> Self {
        Self {
            version: 1,
            maximum_extension_daa: 1_000,
            maximum_consent_window_daa: 100,
            maximum_claim_horizon_daa: 100_000,
            maximum_open_extensions: 64,
            require_price_contract: false,
            security_review_scope: [3; 64],
            economic_review_scope: [4; 64],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwHorizonExtensionFenceV1 {
    pub activation: ForkActivation,
    pub policy: PalwHorizonExtensionPolicyV1,
}

impl PalwHorizonExtensionFenceV1 {
    pub fn unapproved_probe_at(activation: ForkActivation) -> Self {
        Self { activation, policy: PalwHorizonExtensionPolicyV1::unapproved_probe_v1() }
    }
}

/// **The claim-fixed hard deadline** of a kernel claim accepted at `committed_daa` (`d0`): fixed at acceptance, never moved by an
/// extension. `None` on overflow (refused).
// X3: the kernel claim has no native `dispute_hard_deadline`; this is its local claim-fixed bound from its own acceptance. The fold
// caps it with `palw_ruling_fences_v1::palw_claim_effective_deadline_v1(own, palw_claim_hard_deadline_of_v1(root))` — the EXEC root
// native claim's X3 deadline where `palw_claim_hard_deadline_v1` governs it. The lead may replace the local term once X3 defines a
// kernel-claim deadline.
pub fn palw_horizon_extension_hard_deadline_v1(committed_daa: u64, policy: &PalwHorizonExtensionPolicyV1) -> Option<u64> {
    committed_daa.checked_add(policy.maximum_claim_horizon_daa).filter(|d| *d != u64::MAX)
}

/// An extension's consents (aux 126, key `0x01 ‖ terms id`).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwHorizonExtensionRowV1 {
    pub terms: HorizonExtensionTermsV1,
    /// Signed parties, canonical (ascending, distinct), a subset of `terms.required_parties()`.
    pub consented: Vec<Digest>,
    pub first_daa: u64,
    pub applied_daa: Option<u64>,
    pub policy: Hash64,
}

impl PalwHorizonExtensionRowV1 {
    pub fn validate(&self) -> Result<(), &'static str> {
        self.terms.validate()?;
        let required = self.terms.required_parties();
        if self.consented.is_empty()
            || self.consented.windows(2).any(|w| w[0] >= w[1])
            || self.consented.iter().any(|p| !required.contains(p))
            || self.policy == Hash64::default()
            || self.first_daa > self.terms.consent_by_daa
            || self.applied_daa.is_some_and(|at| at < self.first_daa || at > self.terms.consent_by_daa || self.consented != required)
        {
            return Err("a horizon extension row is not its signed parties' consent");
        }
        Ok(())
    }
}

pub fn palw_horizon_extension_key_v1(id: &Digest) -> Vec<u8> {
    prefixed(HORIZON_EXTENSION_PREFIX_V1, id)
}

// ---- C. the minimum-price contract ------------------------------------------------------------------------------------------------

/// **The minimum-price policy** (§15.7 / R8-ECON) — every value POLICY, none approved, none inferred.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwMinimumPricePolicyV1 {
    pub version: u16,
    /// `capital_rate`: the capital's cost per `capital_rate_period_daa`, ppm of the capital.
    pub capital_rate_ppm: u64,
    pub capital_rate_period_daa: u64,
    /// `p_collect_lower`: the PROVEN lower bound of the JOINT native-Final-and-collectible-at-release event, ppm. Zero is admissible
    /// (it is the honest value when no proof exists) and makes every offer UNKNOWN — refused, never a positive verdict.
    pub p_collect_lower_ppm: u32,
    /// Measured `C_upper` per class, excluding capital (compute, recomputation, material/acquisition, carriers/filing, other outcome
    /// costs), strictly ascending by class. A class not listed is unmeasured: refused.
    pub class_cost_upper: Vec<(Digest, u64)>,
    pub maximum_contracts: u32,
    pub maximum_contract_daa: u64,
    /// The measured-cost report the table above comes from (must be a reviewed identity).
    pub cost_report_root: Digest,
    pub economic_review_scope: Digest,
}

impl PalwMinimumPricePolicyV1 {
    pub fn id(&self) -> Hash64 {
        Hash64::from_bytes(object_id(b"MISAKA/PALW/MINIMUM-PRICE-POLICY/V1", self))
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.version != 1
            || self.capital_rate_period_daa == 0
            || self.p_collect_lower_ppm > 1_000_000
            || self.class_cost_upper.is_empty()
            || self.class_cost_upper.len() > 4096
            || self.class_cost_upper.windows(2).any(|w| w[0].0 >= w[1].0)
            || self.maximum_contracts == 0
            || self.maximum_contract_daa == 0
            || self.maximum_contract_daa == u64::MAX
            || self.cost_report_root == [0; 64]
            || self.economic_review_scope == [0; 64]
        {
            return Err("the minimum-price policy needs a period, a measured class table, finite capacity and review identities");
        }
        Ok(())
    }

    /// **NOT A PROPOSAL**: structural fixture values for tests and private profiles (one fixture class, a fixture cost, a fixture
    /// collection floor). No measured cost, collection floor or price is implied.
    pub fn unapproved_probe_v1() -> Self {
        Self {
            version: 1,
            capital_rate_ppm: 100,
            capital_rate_period_daa: 1_000,
            p_collect_lower_ppm: 500_000,
            class_cost_upper: vec![([7; 64], 1_000)],
            maximum_contracts: 64,
            maximum_contract_daa: 10_000,
            cost_report_root: [5; 64],
            economic_review_scope: [6; 64],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwMinimumPriceFenceV1 {
    pub activation: ForkActivation,
    pub policy: PalwMinimumPricePolicyV1,
}

impl PalwMinimumPriceFenceV1 {
    pub fn unapproved_probe_at(activation: ForkActivation) -> Self {
        Self { activation, policy: PalwMinimumPricePolicyV1::unapproved_probe_v1() }
    }
}

/// Why an offer is refused. `Unknown` is the honest verdict when evidence is missing — never a pass.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwMinimumPriceRefusalV1 {
    Unknown(&'static str),
    Unrepresentable,
    BelowMinimum { required: u64, offered: u64 },
    Unfunded { funded: u64, offered: u64 },
    Invalid(&'static str),
}

impl std::fmt::Display for PalwMinimumPriceRefusalV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unknown(why) => write!(f, "UNKNOWN: {why}"),
            Self::Unrepresentable => write!(f, "the quote is above the u64 sompi range (never wrapped or clamped)"),
            Self::BelowMinimum { required, offered } => write!(f, "offer {offered} is below the derived minimum {required}"),
            Self::Unfunded { funded, offered } => write!(f, "the job escrow {funded} does not fund the minimum {offered}"),
            Self::Invalid(why) => write!(f, "{why}"),
        }
    }
}

/// `C_upper` = the class's measured cost + `⌈capital · rate · span / (10^6 · period)⌉` (the capital-time area counted once).
pub fn palw_minimum_price_c_upper_v1(
    policy: &PalwMinimumPricePolicyV1,
    class: &Digest,
    capital_sompi: u64,
    span_daa: u64,
) -> Result<u64, PalwMinimumPriceRefusalV1> {
    let base = policy
        .class_cost_upper
        .binary_search_by(|(c, _)| c.cmp(class))
        .map(|i| policy.class_cost_upper[i].1)
        .map_err(|_| PalwMinimumPriceRefusalV1::Unknown("the class has no measured cost upper bound"))?;
    let area = (capital_sompi as u128)
        .checked_mul(policy.capital_rate_ppm as u128)
        .and_then(|v| v.checked_mul(span_daa as u128))
        .ok_or(PalwMinimumPriceRefusalV1::Unrepresentable)?;
    let capital = area.div_ceil(1_000_000u128 * policy.capital_rate_period_daa as u128);
    let total = (base as u128).checked_add(capital).ok_or(PalwMinimumPriceRefusalV1::Unrepresentable)?;
    u64::try_from(total).map_err(|_| PalwMinimumPriceRefusalV1::Unrepresentable)
}

/// `required_M = ⌈C_upper / p_collect_lower⌉`; refused as UNKNOWN when `p_collect_lower` is zero and the cost positive.
pub fn palw_minimum_price_required_v1(policy: &PalwMinimumPricePolicyV1, c_upper: u64) -> Result<u64, PalwMinimumPriceRefusalV1> {
    if c_upper == 0 {
        return Ok(0);
    }
    if policy.p_collect_lower_ppm == 0 {
        return Err(PalwMinimumPriceRefusalV1::Unknown("no proven joint collection floor: no finite offer finances a positive cost"));
    }
    let required = (c_upper as u128 * 1_000_000).div_ceil(policy.p_collect_lower_ppm as u128);
    u64::try_from(required).map_err(|_| PalwMinimumPriceRefusalV1::Unrepresentable)
}

/// **Admission of a contract offer** at `now` against the job's actual escrow: the policy it names, the window, the derived minimum and
/// the funding. Pure: a refusal writes nothing.
pub fn palw_minimum_price_admit_v1(
    policy: &PalwMinimumPricePolicyV1,
    terms: &MinimumPriceContractTermsV1,
    job_escrow_sompi: u64,
    now: u64,
) -> Result<u64, PalwMinimumPriceRefusalV1> {
    terms.validate().map_err(PalwMinimumPriceRefusalV1::Invalid)?;
    if terms.policy != policy.id().as_bytes() {
        return Err(PalwMinimumPriceRefusalV1::Invalid("the contract names another minimum-price policy"));
    }
    if terms.accept_by_daa < now || terms.retention_until_daa - now > policy.maximum_contract_daa {
        return Err(PalwMinimumPriceRefusalV1::Invalid("the contract's acceptance or retention window is outside the policy"));
    }
    let c_upper = palw_minimum_price_c_upper_v1(policy, &terms.class, terms.capital_sompi, terms.retention_until_daa - now)?;
    let required = palw_minimum_price_required_v1(policy, c_upper)?;
    if terms.funded_minimum_sompi < required {
        return Err(PalwMinimumPriceRefusalV1::BelowMinimum { required, offered: terms.funded_minimum_sompi });
    }
    if job_escrow_sompi < terms.funded_minimum_sompi {
        return Err(PalwMinimumPriceRefusalV1::Unfunded { funded: job_escrow_sompi, offered: terms.funded_minimum_sompi });
    }
    Ok(required)
}

/// A contract's signatures (aux 127, key `0x01 ‖ contract id`).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwMinimumPriceRowV1 {
    pub terms: MinimumPriceContractTermsV1,
    pub consented: Vec<Digest>,
    pub first_daa: u64,
    /// When the last required party signed: from here the contract governs its job.
    pub signed_daa: Option<u64>,
    pub policy: Hash64,
}

impl PalwMinimumPriceRowV1 {
    pub fn validate(&self) -> Result<(), &'static str> {
        self.terms.validate()?;
        let required = self.terms.required_parties();
        if self.consented.is_empty()
            || self.consented.windows(2).any(|w| w[0] >= w[1])
            || self.consented.iter().any(|p| !required.contains(p))
            || self.policy.as_bytes() != self.terms.policy
            || self.first_daa > self.terms.accept_by_daa
            || self.signed_daa.is_some_and(|at| at < self.first_daa || at > self.terms.accept_by_daa || self.consented != required)
        {
            return Err("a minimum-price row is not its signed parties' contract");
        }
        Ok(())
    }
}

pub fn palw_minimum_price_contract_key_v1(id: &Digest) -> Vec<u8> {
    prefixed(MINIMUM_PRICE_CONTRACT_PREFIX_V1, id)
}

pub fn palw_minimum_price_job_key_v1(job: &Digest) -> Vec<u8> {
    prefixed(MINIMUM_PRICE_JOB_PREFIX_V1, job)
}

// ---- the fences, their mirror and refusal ------------------------------------------------------------------------------------------

/// **The three fences, mirrored on the V2 bundle's state params** (the fold reads them). Not Borsh-carried and not hashed — the fences
/// themselves are what the identity ids name (Some-only).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PalwEconContractsMirrorV1 {
    pub model_verification_gate: Option<(u64, PalwModelVerificationGatePolicyV1)>,
    pub horizon_extension: Option<(u64, PalwHorizonExtensionPolicyV1)>,
    pub minimum_price: Option<(u64, PalwMinimumPricePolicyV1)>,
}

impl PalwEconContractsMirrorV1 {
    pub fn model_verification_gate_at(&self, daa: u64) -> Option<&PalwModelVerificationGatePolicyV1> {
        self.model_verification_gate.as_ref().filter(|(from, _)| daa >= *from).map(|(_, p)| p)
    }

    pub fn horizon_extension_at(&self, daa: u64) -> Option<&PalwHorizonExtensionPolicyV1> {
        self.horizon_extension.as_ref().filter(|(from, _)| daa >= *from).map(|(_, p)| p)
    }

    pub fn minimum_price_at(&self, daa: u64) -> Option<&PalwMinimumPricePolicyV1> {
        self.minimum_price.as_ref().filter(|(from, _)| daa >= *from).map(|(_, p)| p)
    }
}

impl crate::palw_state_v2::PalwStateParamsV2 {
    pub fn with_econ_contracts_v1(mut self, mirror: PalwEconContractsMirrorV1) -> Self {
        self.econ_contracts_v1 = mirror;
        self
    }

    pub fn econ_contracts_v1(&self) -> &PalwEconContractsMirrorV1 {
        &self.econ_contracts_v1
    }
}

fn armed(at: ForkActivation) -> Option<u64> {
    (at != ForkActivation::never()).then(|| at.daa_score())
}

impl Params {
    pub fn palw_model_verification_gate_activation(&self) -> Option<u64> {
        self.palw_model_verification_gate_v1.and_then(|f| armed(f.activation))
    }

    pub fn palw_horizon_extension_activation(&self) -> Option<u64> {
        self.palw_horizon_extension_v1.and_then(|f| armed(f.activation))
    }

    pub fn palw_minimum_price_activation(&self) -> Option<u64> {
        self.palw_minimum_price_v1.as_ref().and_then(|f| armed(f.activation))
    }

    /// **Mirror the three fences into the V2 bundle** (the fold holds only the state params). `None` where a fence is not armed. Called
    /// wherever one of them is set on an assembled ruleset (the fork-id probe, the A-2 harness); validation refuses every armed one.
    pub fn sync_palw_econ_contracts_v1(&mut self) {
        let mirror = PalwEconContractsMirrorV1 {
            model_verification_gate: self.palw_model_verification_gate_v1.and_then(|f| armed(f.activation).map(|at| (at, f.policy))),
            horizon_extension: self.palw_horizon_extension_v1.and_then(|f| armed(f.activation).map(|at| (at, f.policy))),
            minimum_price: self.palw_minimum_price_v1.as_ref().and_then(|f| armed(f.activation).map(|at| (at, f.policy.clone()))),
        };
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_econ_contracts_v1(mirror);
        }
    }

    /// **The gate's refusal**: its policy, the allocation fence at or below it by name, then the blanket refusal.
    pub fn validate_palw_model_verification_gate_v1(&self) -> Result<(), PalwModeV2Error> {
        let Some(at) = self.palw_model_verification_gate_activation() else { return Ok(()) };
        let fence = self.palw_model_verification_gate_v1.expect("armed");
        fence.policy.validate().map_err(PalwModeV2Error::Invalid)?;
        let allocation = self.palw_model_bond_allocation_v1.as_ref().and_then(|f| armed(f.activation));
        if allocation.is_none_or(|a| a > at) {
            return Err(PalwModeV2Error::Invalid(
                "palw_model_verification_gate_v1 requires palw_model_bond_allocation_v1 (the allocation it gates) at or below it",
            ));
        }
        Err(PalwModeV2Error::Invalid(
            "palw_model_verification_gate_v1 cannot be armed: the gate size, verifier-pool A-BFT premise, beacon wiring and the ADR-0177 \
             D1 reading remain unapproved",
        ))
    }

    pub fn validate_palw_horizon_extension_v1(&self) -> Result<(), PalwModeV2Error> {
        let Some(at) = self.palw_horizon_extension_activation() else { return Ok(()) };
        let fence = self.palw_horizon_extension_v1.expect("armed");
        fence.policy.validate().map_err(PalwModeV2Error::Invalid)?;
        let exec = self.palw_exec_payload_v2.and_then(armed);
        if exec.is_none_or(|e| e > at) {
            return Err(PalwModeV2Error::Invalid(
                "palw_horizon_extension_v1 requires palw_exec_payload_v2 (the EXEC dependency it extends) at or below it",
            ));
        }
        Err(PalwModeV2Error::Invalid(
            "palw_horizon_extension_v1 cannot be armed: the extension's capital price, provider transport and the X3 unification remain \
             unapproved",
        ))
    }

    pub fn validate_palw_minimum_price_v1(&self) -> Result<(), PalwModeV2Error> {
        let Some(at) = self.palw_minimum_price_activation() else { return Ok(()) };
        let fence = self.palw_minimum_price_v1.as_ref().expect("armed");
        fence.policy.validate().map_err(PalwModeV2Error::Invalid)?;
        let route = self.palw_probabilistic_constraints_v1.and_then(armed);
        if route.is_none_or(|r| r > at) {
            return Err(PalwModeV2Error::Invalid(
                "palw_minimum_price_v1 requires palw_probabilistic_constraints_v1 (the kernel route) at or below it",
            ));
        }
        Err(PalwModeV2Error::Invalid(
            "palw_minimum_price_v1 cannot be armed: measured class costs, the joint collection floor and settlement enforcement remain \
             unapproved",
        ))
    }
}

// ---- restore consistency -----------------------------------------------------------------------------------------------------------

impl crate::palw_state_v2::PalwChainStateV2 {
    /// **Semantic restore fence of aux 125–127.** A row exists only past its fence; every row is its signed parties' consent or the
    /// tally its policy drew; every applied extension is still in the ledger (the horizon at least the consented one, never past the
    /// claim-fixed deadline); every contract index names its contract.
    pub fn assert_econ_contracts_consistency_v1(
        &self,
        params: &crate::palw_state_v2::PalwStateParamsV2,
    ) -> Result<(), crate::palw_state_v2::PalwStateV2Error> {
        use crate::palw_state_v2::PalwStateV2Error;
        let refuse = |why: String| PalwStateV2Error::CarriageInconsistent(format!("econ contracts: {why}"));
        let Some(route) = self.kernel_route() else { return Ok(()) };
        let mirror = params.econ_contracts_v1();
        let mut applied = Vec::new();
        for ((table, key), bytes) in &route.aux {
            match *table {
                PALW_MODEL_VERIFICATION_TABLE_V1 => {
                    let (_, policy) =
                        mirror.model_verification_gate.as_ref().ok_or_else(|| refuse("a tally without its fence".into()))?;
                    let row: PalwModelVerificationTallyV1 = borsh::from_slice(bytes).map_err(|e| refuse(e.to_string()))?;
                    row.validate(policy).map_err(|e| refuse(e.into()))?;
                    if key.as_slice() != palw_model_verification_tally_id_v1(&row.model, row.epoch).as_slice() {
                        return Err(refuse("a tally under another key".into()));
                    }
                }
                PALW_HORIZON_EXTENSION_TABLE_V1 => {
                    let (_, policy) =
                        mirror.horizon_extension.as_ref().ok_or_else(|| refuse("an extension without its fence".into()))?;
                    let row: PalwHorizonExtensionRowV1 = borsh::from_slice(bytes).map_err(|e| refuse(e.to_string()))?;
                    row.validate().map_err(|e| refuse(e.into()))?;
                    if *key != palw_horizon_extension_key_v1(&row.terms.id()) || row.policy != policy.id() {
                        return Err(refuse("an extension under another key or policy".into()));
                    }
                    if row.applied_daa.is_some() {
                        applied.push(row.terms);
                    }
                }
                PALW_MINIMUM_PRICE_TABLE_V1 => {
                    mirror.minimum_price.as_ref().ok_or_else(|| refuse("a contract without its fence".into()))?;
                    match key.first() {
                        Some(&MINIMUM_PRICE_CONTRACT_PREFIX_V1) if key.len() == 65 => {
                            let row: PalwMinimumPriceRowV1 = borsh::from_slice(bytes).map_err(|e| refuse(e.to_string()))?;
                            row.validate().map_err(|e| refuse(e.into()))?;
                            let index = route.aux.get(&(PALW_MINIMUM_PRICE_TABLE_V1, palw_minimum_price_job_key_v1(&row.terms.job)));
                            if *key != palw_minimum_price_contract_key_v1(&row.terms.id())
                                || index.is_none_or(|id| id.as_slice() != row.terms.id().as_slice())
                            {
                                return Err(refuse("a contract under another key or without its job index".into()));
                            }
                        }
                        Some(&MINIMUM_PRICE_JOB_PREFIX_V1) if key.len() == 65 && bytes.len() == 64 => {
                            let id: Digest = bytes.as_slice().try_into().expect("64 bytes");
                            if !route.aux.contains_key(&(PALW_MINIMUM_PRICE_TABLE_V1, palw_minimum_price_contract_key_v1(&id))) {
                                return Err(refuse("a job index names no contract".into()));
                            }
                        }
                        _ => return Err(refuse("an unallocated minimum-price row namespace".into())),
                    }
                }
                _ => {}
            }
        }
        if !applied.is_empty() {
            let ledger = route.ledger().map_err(refuse)?;
            for terms in applied {
                let row = ledger.claims.get(&terms.claim).ok_or_else(|| refuse("an applied extension's claim is missing".into()))?;
                // The horizon may since have been raised by the ordinary `max` rules; it is never below the consented one, and the
                // consented one never passes the claim-fixed deadline.
                if row.liability_until.is_none_or(|until| until < terms.new_liability_until_daa)
                    || terms.new_liability_until_daa > terms.hard_deadline_daa
                    || row.producer != terms.worker
                {
                    return Err(refuse("an applied extension is not in its claim's horizon".into()));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::params::{MAINNET_PARAMS, TESTNET_PARAMS};

    #[test]
    fn the_three_fences_are_dormant_everywhere_refused_when_armed_and_hashed_some_only() {
        for p in [MAINNET_PARAMS, TESTNET_PARAMS] {
            assert_eq!(p.palw_model_verification_gate_v1, None);
            assert_eq!(p.palw_horizon_extension_v1, None);
            assert_eq!(p.palw_minimum_price_v1, None);
            p.validate_palw_model_verification_gate_v1().unwrap();
            p.validate_palw_horizon_extension_v1().unwrap();
            p.validate_palw_minimum_price_v1().unwrap();
            for name in ["palw_model_verification_gate_v1", "palw_horizon_extension_v1", "palw_minimum_price_v1"] {
                assert!(p.palw_fences_v1().contains(&(name, None)), "{name}: the exhaustive list names it");
            }
        }
        let mut p = TESTNET_PARAMS;
        let (id, schedule) = (p.consensus_params_id(), p.consensus_schedule_id());
        p.palw_model_verification_gate_v1 = Some(PalwModelVerificationGateFenceV1::unapproved_probe_at(ForkActivation::never()));
        p.palw_horizon_extension_v1 = Some(PalwHorizonExtensionFenceV1::unapproved_probe_at(ForkActivation::never()));
        p.palw_minimum_price_v1 = Some(PalwMinimumPriceFenceV1::unapproved_probe_at(ForkActivation::never()));
        p.validate_palw_model_verification_gate_v1().unwrap();
        p.validate_palw_horizon_extension_v1().unwrap();
        p.validate_palw_minimum_price_v1().unwrap();
        let at = ForkActivation::new(1_000);
        p.palw_model_verification_gate_v1 = Some(PalwModelVerificationGateFenceV1::unapproved_probe_at(at));
        p.palw_horizon_extension_v1 = Some(PalwHorizonExtensionFenceV1::unapproved_probe_at(at));
        p.palw_minimum_price_v1 = Some(PalwMinimumPriceFenceV1::unapproved_probe_at(at));
        assert!(p.validate_palw_model_verification_gate_v1().is_err());
        assert!(p.validate_palw_horizon_extension_v1().is_err());
        assert!(p.validate_palw_minimum_price_v1().is_err());
        assert_ne!(p.consensus_params_id(), id, "an armed height is a different network");
        assert_ne!(p.consensus_schedule_id(), schedule);
    }

    #[test]
    fn the_probe_policies_are_structurally_valid_and_the_invariants_refuse_their_edges() {
        PalwModelVerificationGatePolicyV1::unapproved_probe_v1().validate().unwrap();
        PalwHorizonExtensionPolicyV1::unapproved_probe_v1().validate().unwrap();
        PalwMinimumPricePolicyV1::unapproved_probe_v1().validate().unwrap();
        let mut g = PalwModelVerificationGatePolicyV1::unapproved_probe_v1();
        g.threshold = 23;
        assert!(g.validate().is_err(), "2k > m: 23 of 47 is not a majority");
        g.threshold = 48;
        assert!(g.validate().is_err(), "k <= m");
        let mut m = PalwMinimumPricePolicyV1::unapproved_probe_v1();
        m.class_cost_upper = vec![([8; 64], 1), ([7; 64], 1)];
        assert!(m.validate().is_err(), "the class table is canonical");
    }

    #[test]
    fn the_draw_is_stake_weighted_deterministic_canonical_and_split_neutral() {
        let pool = [([1; 64], 30), ([2; 64], 70)];
        let a = palw_model_verification_draw_v1(&[9; 64], &[8; 64], &pool, 47).unwrap();
        assert_eq!(a, palw_model_verification_draw_v1(&[9; 64], &[8; 64], &pool, 47).unwrap(), "deterministic");
        assert_eq!(a.len(), 47);
        assert!(palw_model_verification_draw_v1(&[9; 64], &[8; 64], &[([2; 64], 70), ([1; 64], 30)], 47).is_err(), "canonical");
        assert!(palw_model_verification_draw_v1(&[9; 64], &[8; 64], &[([1; 64], 0)], 1).is_err(), "positive stakes");
        // A stake split into two adjacent bonds of the same total covers the same draw points.
        let split = [([1; 64], 30), ([2; 64], 35), ([3; 64], 35)];
        let b = palw_model_verification_draw_v1(&[9; 64], &[8; 64], &split, 47).unwrap();
        for (x, y) in a.iter().zip(&b) {
            assert_eq!(*x == [1; 64], *y == [1; 64], "the first bond's points are the same with the rest split");
        }
    }

    fn tally(sampled: bool, verdicts: &[ModelVerificationVerdictV1]) -> PalwModelVerificationTallyV1 {
        PalwModelVerificationTallyV1 {
            model: [1; 64],
            epoch: 3,
            beacon: [2; 64],
            model_root: [4; 64],
            sampled_claim: sampled.then_some([3; 64]),
            slots: verdicts.iter().enumerate().map(|(i, v)| ([i as u8 + 10; 64], *v)).collect(),
            opened_daa: 100,
            deadline_daa: 110,
            policy: Hash64::default(),
        }
    }

    #[test]
    fn the_gate_withholds_only_unattested_verification_never_an_unserved_model() {
        use ModelVerificationVerdictV1::*;
        use PalwModelVerificationOutcomeV1 as O;
        let t = tally(true, &[Verified, Verified, Unset, Unset, Unset]);
        assert_eq!(t.outcome(3, 105), O::Pending);
        assert_eq!(t.outcome(3, 111), O::Unattested, "past the deadline");
        assert_eq!(t.outcome(2, 105), O::Attested);
        let t = tally(true, &[CouldNotVerify, CouldNotVerify, CouldNotVerify, Verified, Unset]);
        assert_eq!(t.outcome(3, 105), O::Unattested, "k can no longer be reached");
        let t = tally(false, &[Unset; 5]);
        assert_eq!(t.outcome(3, 1_000), O::NoFinalClaim, "no sample: never a withholding reason");
        assert_eq!(palw_model_verification_subsidy_v1(O::NoFinalClaim, 50).withheld, 0);
        assert_eq!(palw_model_verification_subsidy_v1(O::Unattested, 50), PalwGatedSubsidyV1 { withheld: 50, ..Default::default() });
        assert_eq!(palw_model_verification_subsidy_v1(O::Pending, 50).deferred, 50);
        assert_eq!(palw_model_verification_subsidy_v1(O::Attested, 50).payable, 50);
        assert_eq!(
            palw_model_verification_sample_v1(PalwVerificationSampleRuleV1::UniformFinalClaimV1, &[1; 64], &[2; 64], &[]),
            Ok(None)
        );
        assert!(
            palw_model_verification_sample_v1(
                PalwVerificationSampleRuleV1::UniformFinalClaimV1,
                &[1; 64],
                &[2; 64],
                &[[2; 64], [1; 64]]
            )
            .is_err()
        );
    }

    fn contract(policy: &PalwMinimumPricePolicyV1, funded: u64) -> MinimumPriceContractTermsV1 {
        MinimumPriceContractTermsV1 {
            version: 1,
            network: [1; 64],
            worker: [2; 64],
            poster: [3; 64],
            job: [4; 64],
            class: [7; 64],
            policy: policy.id().as_bytes(),
            funded_minimum_sompi: funded,
            capital_sompi: 1_000_000,
            accept_by_daa: 150,
            retention_until_daa: 1_100,
        }
    }

    #[test]
    fn a_minimum_price_offer_one_sompi_short_unmeasured_unfunded_or_uncollectible_is_refused() {
        let p = PalwMinimumPricePolicyV1::unapproved_probe_v1();
        // C_upper = 1,000 + ⌈10^6 · 100 · 1,000 / (10^6 · 1,000)⌉ = 1,100; required = ⌈1,100 / 0.5⌉ = 2,200.
        assert_eq!(palw_minimum_price_c_upper_v1(&p, &[7; 64], 1_000_000, 1_000), Ok(1_100));
        assert_eq!(palw_minimum_price_admit_v1(&p, &contract(&p, 2_200), 2_200, 100), Ok(2_200));
        assert_eq!(
            palw_minimum_price_admit_v1(&p, &contract(&p, 2_199), 10_000, 100),
            Err(PalwMinimumPriceRefusalV1::BelowMinimum { required: 2_200, offered: 2_199 })
        );
        assert_eq!(
            palw_minimum_price_admit_v1(&p, &contract(&p, 2_200), 2_199, 100),
            Err(PalwMinimumPriceRefusalV1::Unfunded { funded: 2_199, offered: 2_200 })
        );
        let mut unmeasured = contract(&p, 2_200);
        unmeasured.class = [8; 64];
        assert!(matches!(palw_minimum_price_admit_v1(&p, &unmeasured, 10_000, 100), Err(PalwMinimumPriceRefusalV1::Unknown(_))));
        let mut zero = p.clone();
        zero.p_collect_lower_ppm = 0;
        let mut c = contract(&zero, 2_200);
        c.policy = zero.id().as_bytes();
        assert!(matches!(palw_minimum_price_admit_v1(&zero, &c, 10_000, 100), Err(PalwMinimumPriceRefusalV1::Unknown(_))));
        assert_eq!(palw_minimum_price_c_upper_v1(&p, &[7; 64], u64::MAX, u64::MAX), Err(PalwMinimumPriceRefusalV1::Unrepresentable));
    }
}
