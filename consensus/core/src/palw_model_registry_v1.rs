//! **ADR-0135 — the permissionless model registry: a model is data, its profile is derived, its
//! panel proves readiness, and its lifecycle is a state machine.** Shadow: nothing consensus
//! reads yet; the types and functions here are the rule set Protocol Upgrade A arms.
//!
//! What changes from ADR-0133: nothing a human measures enters consensus. A registrant submits a
//! [`PalwModelManifestV1`] (graph root, artifact root and bytes, the canonical job, the
//! quantization, the runtime version) and a registration bond; every node derives the same
//! [`PalwDerivedProfileV1`] from the manifest's *work* — the verification compute the graph costs
//! (ADR-0131's economic compute of the draw job) and the artifact's bytes — against one set of
//! global reference constants ([`PalwRegistryGlobalsV1`]); panels prove readiness with evidence,
//! not declarations ([`PalwReadinessEvidenceV1`]); the class walks a lifecycle
//! ([`PalwModelLifecycleV1`]) whose every transition is a function of chain-visible facts; and a
//! class's claim cadence comes from a global compute budget over its economic compute
//! ([`palw_admission_claims_per_span_v1`]), so no share is set by anyone. Wall-clock p99s are
//! telemetry: they say whether the derived profile is being met, never what it is.
//!
//! The boundary that stays: a model the canonical ML VM can express (matmul, attention, MoE, GDN,
//! norms, …) registers without a fork; a model that needs an instruction the VM lacks is a VM
//! upgrade (`PalwManifestVerdictV1::UnsupportedOp`), not a registration.

use crate::Hash64;
use crate::palw_verification_profile_v1::{PALW_SEAT_COUNT_V1, PALW_SPAN_MS_V1};
use std::collections::BTreeMap;

pub const PALW_MODEL_REGISTRY_VERSION_V1: u16 = 1;

/// **What a registrant submits.** No window, no reward, no share: those are derived.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwModelManifestV1 {
    pub graph_ir_root: Hash64,
    pub artifact_root: Hash64,
    pub artifact_bytes: u64,
    pub canonical_prefill_tokens: u32,
    pub canonical_decode_tokens: u32,
    pub quantization_format: u8,
    pub runtime_version: u16,
}

/// **What the graph costs, derived by every node from the manifest's graph** (ADR-0131's economic
/// compute): the compute of the job a seat replays to verify one claim, and the working set the
/// replay touches (the artifact whole for a dense model, the resident experts for a mixture).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwModelWorkV1 {
    pub verification_ccu: u128,
    pub economic_ccu_per_claim: u128,
    pub artifact_bytes: u64,
    pub working_set_bytes: u64,
    /// Whether every op of the graph is one the canonical VM defines at `runtime_version`.
    pub ops_supported: bool,
}

/// **The global reference every model is measured against** — consensus constants once Protocol
/// Upgrade A is armed, the same for every class; changed only by a fence, never per model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwRegistryGlobalsV1 {
    /// MAC-equivalents a reference seat verifies in one execution span.
    pub reference_work_per_span: u128,
    /// Bytes a reference seat pages in from storage in one span.
    pub reference_bytes_per_span: u64,
    pub safety_permille: u32,
    pub io_safety_permille: u32,
    /// Spans allowed for a receipt to propagate and be carried.
    pub receipt_allowance_spans: u32,
    pub seat_count: u16,
    /// Ready seats beyond the panel a class must have before it is live.
    pub spare_seats: u16,
    pub utilization_permille: u32,
    /// Probation: the probe claims a class must finalize with none failing.
    pub probation_claims: u32,
    /// Epochs a limited class must run at target before it is `Active`.
    pub stable_epochs: u32,
    /// The registration bond: a base per span of verification burden.
    pub registration_bond_per_span_sompi: u64,
    /// The compute a span's claims may cost the network, all classes together.
    pub budget_ccu_per_span: u128,
    /// Free collateral a ready seat must hold, as a multiple of the class's seat exposure.
    pub readiness_collateral_multiple: u32,
    /// How recently a seat's probe verification must have succeeded, in spans.
    pub readiness_probe_max_age_spans: u32,
}

/// The reference the fleet measured (ADR-0133): 4 G MAC-eq/s → 2.4 T a span; 1 GB/s → 600 GB a
/// span; ×2 / ×2; one span of receipt allowance; five seats plus two spare; 70 %; ten probe
/// claims; three stable epochs; 1,000 MSK a span of burden; a budget of one dense-tier claim's
/// attempted compute a span times ten; three exposures of free collateral; a probe within 30 spans.
pub const PALW_REGISTRY_GLOBALS_V1: PalwRegistryGlobalsV1 = PalwRegistryGlobalsV1 {
    reference_work_per_span: 4_000_000_000 * (PALW_SPAN_MS_V1 as u128) / 1_000,
    reference_bytes_per_span: 1_000_000_000 * PALW_SPAN_MS_V1 / 1_000,
    safety_permille: 2_000,
    io_safety_permille: 2_000,
    receipt_allowance_spans: 1,
    seat_count: PALW_SEAT_COUNT_V1,
    spare_seats: 2,
    utilization_permille: 700,
    probation_claims: 10,
    stable_epochs: 3,
    registration_bond_per_span_sompi: 1_000 * 100_000_000,
    budget_ccu_per_span: 10 * 166_204_342_272,
    readiness_collateral_multiple: 3,
    readiness_probe_max_age_spans: 30,
};

/// A manifest is refused before it is a class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwManifestVerdictV1 {
    Valid,
    /// The graph names an op the canonical VM does not define at this runtime version: a VM
    /// instruction upgrade, not a registration.
    UnsupportedOp,
    EmptyArtifact,
    EmptyJob,
    ZeroWork,
}

pub fn palw_manifest_verdict_v1(manifest: &PalwModelManifestV1, work: &PalwModelWorkV1) -> PalwManifestVerdictV1 {
    if !work.ops_supported {
        PalwManifestVerdictV1::UnsupportedOp
    } else if manifest.artifact_bytes == 0 || work.artifact_bytes == 0 {
        PalwManifestVerdictV1::EmptyArtifact
    } else if manifest.canonical_prefill_tokens == 0 {
        PalwManifestVerdictV1::EmptyJob
    } else if work.verification_ccu == 0 || work.economic_ccu_per_claim == 0 {
        PalwManifestVerdictV1::ZeroWork
    } else {
        PalwManifestVerdictV1::Valid
    }
}

/// **The derived profile.** Every field a function of the manifest's work and the globals; no
/// field a registrant states, no field a human measures.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwDerivedProfileV1 {
    pub version: u16,
    pub verification_window_spans: u32,
    pub artifact_prefetch_spans: u32,
    pub max_inflight_claims: u32,
    pub required_ready_seats: u32,
    pub registration_bond_sompi: u64,
    /// The claims a span the class may accept under the global budget, in thousandths.
    pub admission_claims_per_span_milli: u64,
}

fn ceil_div_u128(a: u128, b: u128) -> u128 {
    if b == 0 { 0 } else { a.div_ceil(b) }
}

/// `⌈safety × work / reference⌉` + the receipt allowance, one span at least.
pub fn palw_verification_window_spans_v1(work: &PalwModelWorkV1, g: &PalwRegistryGlobalsV1) -> u32 {
    let spans = ceil_div_u128(
        work.verification_ccu.saturating_mul(g.safety_permille.max(1_000) as u128),
        g.reference_work_per_span.saturating_mul(1_000),
    );
    (spans.max(1) + g.receipt_allowance_spans as u128).min(u32::MAX as u128) as u32
}

/// `⌈io_safety × bytes / reference_bytes⌉`, zero for a model with nothing to page.
pub fn palw_artifact_prefetch_spans_v1(work: &PalwModelWorkV1, g: &PalwRegistryGlobalsV1) -> u32 {
    if work.artifact_bytes == 0 {
        return 0;
    }
    let spans = ceil_div_u128(
        (work.artifact_bytes as u128).saturating_mul(g.io_safety_permille.max(1_000) as u128),
        (g.reference_bytes_per_span as u128).saturating_mul(1_000),
    );
    spans.max(1).min(u32::MAX as u128) as u32
}

/// The seats a class needs before it is live: the panel plus the spare, or more where one claim a
/// span at the target utilization already needs more replay time than that many seats offer.
pub fn palw_required_ready_seats_v1(work: &PalwModelWorkV1, g: &PalwRegistryGlobalsV1) -> u32 {
    let floor = g.seat_count as u128 + g.spare_seats as u128;
    let per_claim = work.verification_ccu.saturating_mul(g.seat_count as u128);
    let offered_per_seat = g.reference_work_per_span.saturating_mul(g.utilization_permille.min(1_000) as u128) / 1_000;
    let for_one_a_span = ceil_div_u128(per_claim, offered_per_seat.max(1));
    floor.max(for_one_a_span).min(u32::MAX as u128) as u32
}

/// Little's law at the target utilization over the required seats: the claims that may be in
/// flight over the window, one at least.
pub fn palw_max_inflight_claims_v1(work: &PalwModelWorkV1, g: &PalwRegistryGlobalsV1) -> u32 {
    let window = palw_verification_window_spans_v1(work, g) as u128;
    let seats = palw_required_ready_seats_v1(work, g) as u128;
    let offered = seats
        .saturating_mul(window)
        .saturating_mul(g.reference_work_per_span)
        .saturating_mul(g.utilization_permille.min(1_000) as u128)
        / 1_000;
    let per_claim = work.verification_ccu.saturating_mul(g.seat_count as u128).max(1);
    (offered / per_claim).clamp(1, u32::MAX as u128) as u32
}

/// **Admission without a share**: the claims a span the global budget affords this class,
/// `budget / economic compute a claim` — a heavy class claims rarely and is paid much per claim, a
/// light class often and little, and the budget is what is constant. Capped at what the window
/// can hold (`max_inflight / window`). In thousandths.
pub fn palw_admission_claims_per_span_v1(work: &PalwModelWorkV1, g: &PalwRegistryGlobalsV1) -> u64 {
    if work.economic_ccu_per_claim == 0 {
        return 0;
    }
    let by_budget = g.budget_ccu_per_span.saturating_mul(1_000) / work.economic_ccu_per_claim;
    let by_capacity = (palw_max_inflight_claims_v1(work, g) as u128).saturating_mul(1_000)
        / palw_verification_window_spans_v1(work, g).max(1) as u128;
    by_budget.min(by_capacity).min(u64::MAX as u128) as u64
}

/// The whole profile.
pub fn palw_derive_profile_v1(work: &PalwModelWorkV1, g: &PalwRegistryGlobalsV1) -> PalwDerivedProfileV1 {
    let verification_window_spans = palw_verification_window_spans_v1(work, g);
    PalwDerivedProfileV1 {
        version: PALW_MODEL_REGISTRY_VERSION_V1,
        verification_window_spans,
        artifact_prefetch_spans: palw_artifact_prefetch_spans_v1(work, g),
        max_inflight_claims: palw_max_inflight_claims_v1(work, g),
        required_ready_seats: palw_required_ready_seats_v1(work, g),
        registration_bond_sompi: g.registration_bond_per_span_sompi.saturating_mul(verification_window_spans as u64),
        admission_claims_per_span_milli: palw_admission_claims_per_span_v1(work, g),
    }
}

/// **The single lottery's class targets, from admission alone** (ADR-0132 S, ADR-0133 §9a Fence 2):
/// each class's share of draws is its admitted claims a span over every class's, in permille —
/// no share is set by anyone, and a class whose admission is zero holds no share.
pub fn palw_class_shares_from_admission_v1(admissions_milli: &[(Hash64, u64)]) -> Vec<(Hash64, u16)> {
    let total: u128 = admissions_milli.iter().map(|(_, a)| *a as u128).sum();
    admissions_milli
        .iter()
        .map(|(class, a)| (*class, if total == 0 { 0 } else { ((*a as u128).saturating_mul(1_000) / total).min(1_000) as u16 }))
        .collect()
}

/// **What a seat proves before it counts as ready for a class** — none of it a declaration.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwReadinessEvidenceV1 {
    /// The seat's held artifact root equals the manifest's.
    pub artifact_root_matches: bool,
    /// Every chunk of the artifact's manifest is held (a possession proof over the root).
    pub all_chunks_held: bool,
    /// The seat's node is synced, mining-eligible and not held (`participation_allowed`).
    pub participation_ok: bool,
    pub free_collateral_sompi: u128,
    /// The span of the seat's last successful canonical probe verification on this class, if any.
    pub last_probe_ok_span: Option<u64>,
}

impl PalwReadinessEvidenceV1 {
    /// Ready: root matched, chunks held, participating, collateral for `readiness_collateral_multiple`
    /// seat exposures, and a probe verified within `readiness_probe_max_age_spans`.
    pub fn is_ready(&self, seat_exposure_sompi: u128, now_span: u64, g: &PalwRegistryGlobalsV1) -> bool {
        let collateral_needed = seat_exposure_sompi.saturating_mul(g.readiness_collateral_multiple as u128);
        let probe_fresh =
            self.last_probe_ok_span.is_some_and(|at| now_span.saturating_sub(at) <= g.readiness_probe_max_age_spans as u64);
        self.artifact_root_matches
            && self.all_chunks_held
            && self.participation_ok
            && self.free_collateral_sompi >= collateral_needed
            && probe_fresh
    }
}

/// **The lifecycle.** Every transition a function of chain-visible facts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PalwModelLifecycleV1 {
    Registered,
    Prefetching,
    Probation {
        probes_passed: u32,
    },
    ActiveLimited {
        stable_epochs: u32,
    },
    Active,
    /// A class whose panel cannot be drawn or whose utilization passed one: its own new claims
    /// hold; nothing else stops.
    Held,
}

impl PalwModelLifecycleV1 {
    /// Whether the class accepts new claims at all.
    pub fn admits_claims(&self) -> bool {
        matches!(self, Self::Probation { .. } | Self::ActiveLimited { .. } | Self::Active)
    }
    /// The fraction of the derived admission the state allows, in permille: probation runs the
    /// probe claims only, limited activation a tenth, `Active` all of it.
    /// The cadence a state admits, in permille of the derived admission: probation at a twentieth
    /// (a class must be able to produce the claims that probe it — at zero it could never leave
    /// probation), limited activation at a tenth, activation in full, everything else nothing.
    pub fn admission_permille(&self) -> u32 {
        match self {
            Self::Probation { .. } => 50,
            Self::ActiveLimited { .. } => 100,
            Self::Active => 1_000,
            _ => 0,
        }
    }
}

/// What the chain sees of a class at a span boundary.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwLifecycleObservationV1 {
    pub manifest: PalwManifestVerdictV1Flag,
    pub ready_seats: u32,
    pub probes_passed_this_span: u32,
    pub probes_failed_this_span: u32,
    pub utilization_permille: u32,
    pub collateral_ok: bool,
    /// ADR-0132 Upgrade C: the class's cap utilization is at or under the fence's ceiling (`true`
    /// where the economic payout is dormant: nothing prices it). A class over it is not stepped to
    /// `Active` and an `Active` one falls back to a tenth — "not activatable", as a rule.
    pub cap_ok: bool,
    /// Whether this span ran at or under the target utilization with no held claim.
    pub span_stable: bool,
}

/// `PalwManifestVerdictV1`, as a flag the observation carries (`Valid` or not).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PalwManifestVerdictV1Flag {
    #[default]
    Invalid,
    Valid,
}

/// One step of the lifecycle at a span boundary.
pub fn palw_lifecycle_step_v1(
    state: PalwModelLifecycleV1,
    obs: &PalwLifecycleObservationV1,
    profile: &PalwDerivedProfileV1,
    g: &PalwRegistryGlobalsV1,
) -> PalwModelLifecycleV1 {
    use PalwModelLifecycleV1::*;
    let panel_drawable = obs.ready_seats >= g.seat_count as u32;
    let ready_enough = obs.ready_seats >= profile.required_ready_seats && obs.collateral_ok;
    let overloaded = obs.utilization_permille >= 1_000;
    match state {
        Registered => {
            if obs.manifest == PalwManifestVerdictV1Flag::Valid {
                Prefetching
            } else {
                Registered
            }
        }
        Prefetching => {
            if ready_enough {
                Probation { probes_passed: 0 }
            } else {
                Prefetching
            }
        }
        Probation { probes_passed } => {
            if !panel_drawable || overloaded {
                Held
            } else if obs.probes_failed_this_span > 0 {
                Probation { probes_passed: 0 }
            } else {
                let passed = probes_passed.saturating_add(obs.probes_passed_this_span);
                if passed >= g.probation_claims && ready_enough {
                    ActiveLimited { stable_epochs: 0 }
                } else {
                    Probation { probes_passed: passed }
                }
            }
        }
        ActiveLimited { stable_epochs } => {
            if !panel_drawable || overloaded {
                Held
            } else if obs.span_stable {
                let stable = stable_epochs.saturating_add(1);
                // ADR-0132 Upgrade C: a cap-saturated class keeps its stable count but is not
                // activated — it admits at a tenth until the rate, the target or the escrow moves.
                if stable >= g.stable_epochs && obs.cap_ok { Active } else { ActiveLimited { stable_epochs: stable } }
            } else {
                ActiveLimited { stable_epochs: 0 }
            }
        }
        Active => {
            if !panel_drawable || overloaded {
                Held
            } else if !obs.cap_ok {
                ActiveLimited { stable_epochs: 0 }
            } else {
                Active
            }
        }
        Held => {
            if ready_enough && !overloaded {
                Probation { probes_passed: 0 }
            } else {
                Held
            }
        }
    }
}

// ---- Protocol Upgrade A: what the fold stores and reads ---------------------------------------

/// **The work of a registered class, read off the carriage its registration rode** (ADR-0067: a
/// class is chain data; the `admission` carriage holds the graph profile and the canonical job).
/// `verification_ccu` is the economic compute of the canonical job a seat replays to verify one
/// claim (ADR-0131's cost table over the graph); `economic_ccu_per_claim` is the compute of ONE
/// draw (the prefill draw job) — the cadence step multiplies it by the class's expected draws a
/// claim (Q32, from its live target), so the profile follows the target and the graph, never a
/// declaration. `artifact_bytes` is an estimate from the graph's dense weights (two bytes a weight
/// at the per-token dense MAC count), used only for the prefetch allowance — **never for admission,
/// the bond, the window or the inflight cap** (those read the compute); a manifest that carries the
/// bytes is V2's, and until then no rule may start reading this field.
pub fn palw_model_work_from_carriage_v1(
    profile: &crate::palw_step::PalwShapeProfileV3,
    canonical: &crate::palw_v2::PalwJobContextV2,
) -> Option<PalwModelWorkV1> {
    use crate::palw_economic_compute_v1::{
        PALW_ECONOMIC_COST_TABLE_V1, palw_attempt_economic_compute_v1, palw_economic_shape_v1, palw_job_economic_compute_v1,
    };
    let table = &PALW_ECONOMIC_COST_TABLE_V1;
    let verification_ccu = palw_job_economic_compute_v1(profile, canonical, table).ok()?;
    let draw = palw_attempt_economic_compute_v1(profile, canonical, true, table).ok()?;
    let shape = palw_economic_shape_v1(profile, table).ok()?;
    let body = shape.body_at(0);
    let artifact_bytes = body.dense_matmul.saturating_add(body.routed_experts).saturating_mul(2).min(u64::MAX as u128) as u64;
    Some(PalwModelWorkV1 {
        verification_ccu,
        economic_ccu_per_claim: draw,
        artifact_bytes,
        working_set_bytes: artifact_bytes,
        ops_supported: verification_ccu > 0,
    })
}

/// **The genesis classes' work**, from the registrations the bundle's genesis carries — every node
/// derives the bundle from `Params`, so this map is a function of the ruleset, not of a store.
pub fn palw_genesis_model_works_v1(objects: &[crate::palw_state_v2::PalwConsensusObjectV2]) -> BTreeMap<Hash64, PalwModelWorkV1> {
    let mut out = BTreeMap::new();
    for object in objects {
        if let crate::palw_state_v2::PalwConsensusObjectV2::ClassRegistered { class_id, admission: Some(carriage), .. } = object
            && let Some(work) = palw_model_work_from_carriage_v1(&carriage.profile, &carriage.canonical)
        {
            out.insert(*class_id, work);
        }
    }
    out
}

/// **The profile a lifecycle row carries this span**: the stored work with the class's expected
/// draws a claim (Q32) folded into the economic compute a claim costs.
pub fn palw_lifecycle_profile_v1(
    work: &PalwModelWorkV1,
    expected_attempts_q32: u128,
    g: &PalwRegistryGlobalsV1,
) -> PalwDerivedProfileV1 {
    let per_claim = crate::palw_economic_compute_v1::palw_attempted_compute_q32_per_claim_v1(
        expected_attempts_q32.max(crate::palw_economic_compute_v1::PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1),
        work.economic_ccu_per_claim,
    );
    palw_derive_profile_v1(&PalwModelWorkV1 { economic_ccu_per_claim: per_claim, ..*work }, g)
}

/// **A class's row in the registry** — the state machine's position, the work its registration
/// derived, the profile of the last span, and the counters the lifecycle reads.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwModelLifecycleRowV1 {
    pub state: PalwModelLifecycleV1,
    pub work: PalwModelWorkV1,
    pub profile: PalwDerivedProfileV1,
    /// The span the current state was entered at.
    pub since_span: u64,
    /// Finals and faults of the class's claims since the state was entered (probation reads them).
    pub probes_passed: u32,
    pub probes_failed: u32,
    /// Finals and faults of the span being stepped, cleared at every boundary.
    pub probes_passed_this_span: u32,
    pub probes_failed_this_span: u32,
    /// The last boundary's reading, kept so a reader (op 186) sees what the step saw.
    pub ready_seats: u32,
    pub inflight_claims: u32,
    pub utilization_permille: u32,
    pub admission_milli: u64,
    /// ADR-0132 Upgrade C: the class's cap utilization at the last boundary, in permille of the
    /// escrow (`attempted × rate / escrow`); `0` where nothing priced it (the payout fence dormant,
    /// or no subsidy at the boundary block). Over the fence's ceiling the class is not activatable.
    pub cap_utilization_permille: u32,
}

/// **A seat's readiness for a class**: the last possession proof this bond opened for the class
/// (ADR-0135 Decision 4). Fresh while `proved_daa` is within the readiness age; the probe half of
/// readiness is the seat's last `Valid` receipt on the class, read off the claims.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwSeatReadinessRowV1 {
    pub proved_daa: u64,
    pub proved_span: u64,
    pub leaf_index: u32,
}

/// **What the fold is handed when the registry is active at a block** (`PalwTransitionExtrasV1`):
/// the globals, the span clock, and the genesis classes' work (the classes registered before the
/// fence, whose carriages the state never stored).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwModelRegistryFoldV1 {
    pub globals: PalwRegistryGlobalsV1,
    pub span_daa: u64,
    pub genesis_works: BTreeMap<Hash64, PalwModelWorkV1>,
    /// **The activation grace** (ADR-0135 §7): proofs are refused below the fence, so at activation
    /// no seat is ready and every live class would be HELD at the first boundary. Until this DAA
    /// (the fence plus one readiness age) the rows open and proofs are taken but no row is stepped
    /// and the draw keeps judging by declaration; from it the registry governs. `0` = no grace.
    pub grace_until_daa: u64,
}

impl PalwModelRegistryFoldV1 {
    /// Whether the registry governs (steps rows, judges by evidence) at `daa_score`.
    pub fn governs_at(&self, daa_score: u64) -> bool {
        daa_score >= self.grace_until_daa
    }

    /// One readiness age past the fence: the grace every activation gets.
    pub fn grace_until_v1(activation_daa: u64, span_daa: u64, globals: &PalwRegistryGlobalsV1) -> u64 {
        activation_daa.saturating_add((globals.readiness_probe_max_age_spans as u64).saturating_mul(span_daa.max(1)))
    }
}

/// **The draw's readiness policy** under the registry: a seat may judge a class only with a
/// possession proof no older than `max_age_daa` at `now_daa` (the base class stays open to every
/// bond, as before — every node runs it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwReadinessPolicyV1 {
    pub now_daa: u64,
    pub max_age_daa: u64,
    pub base_class_id: Hash64,
}

/// One opening per proof, capped: an artifact leaf is a tensor row range and rides the object
/// chunk carriage above a block's payload, so the cap bounds what a proof may ask the chain to carry.
pub const PALW_READINESS_OPENING_MAX_BYTES_V1: usize = 8 << 20;
/// The challenge names `width` consecutive leaves from a seeded start; the prover opens ONE of
/// them (V1: a leaf may be large, so the prover picks the one it can carry). A holder of the whole
/// artifact answers every span; a holder of a fraction fails a span with the fraction it lacks.
pub const PALW_READINESS_CHALLENGE_WIDTH_V1: u32 = 8;
pub const PALW_SEAT_READINESS_V1_DOMAIN: &[u8] = b"misaka-palw/seat-readiness-v1/message/v1";
pub const PALW_SEAT_READINESS_V1_CHALLENGE_DOMAIN: &[u8] = b"misaka-palw/seat-readiness-v1/challenge/v1";
pub const PALW_SEAT_READINESS_V1_MLDSA87_CONTEXT: &[u8] = b"misaka-palw/seat-readiness-v1/mldsa87/v1";

fn keyed64(domain: &[u8]) -> blake2b_simd::State {
    blake2b_simd::Params::new().hash_length(64).key(domain).to_state()
}
fn finish64(state: blake2b_simd::State) -> Hash64 {
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// **The challenge seed of (class, bond, span)**: every node derives it, the prover cannot choose it.
pub fn palw_readiness_challenge_seed_v1(class_id: &Hash64, bond: &[u8], span: u64) -> Hash64 {
    let mut state = keyed64(PALW_SEAT_READINESS_V1_CHALLENGE_DOMAIN);
    state.update(class_id.as_byte_slice());
    state.update(&(bond.len() as u64).to_le_bytes());
    state.update(bond);
    state.update(&span.to_le_bytes());
    finish64(state)
}

/// **The leaves the challenge names**: `(start, width)` over `leaf_count`, wrapping.
pub fn palw_readiness_window_v1(seed: &Hash64, leaf_count: u32) -> (u32, u32) {
    if leaf_count == 0 {
        return (0, 0);
    }
    let bytes = seed.as_bytes();
    let word = u64::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]]);
    ((word % leaf_count as u64) as u32, PALW_READINESS_CHALLENGE_WIDTH_V1.min(leaf_count))
}

/// Whether `leaf_index` is inside the window `(start, width)` of a `leaf_count`-leaf inventory.
pub fn palw_readiness_window_contains_v1(start: u32, width: u32, leaf_count: u32, leaf_index: u32) -> bool {
    if leaf_count == 0 || width == 0 || leaf_index >= leaf_count {
        return false;
    }
    let offset = (leaf_index as u64 + leaf_count as u64 - start as u64) % leaf_count as u64;
    offset < width as u64
}

/// **What a seat signs** to prove readiness: the network, its bond, the class, the span and the
/// leaf it opened — so a relayer cannot volunteer another bond's collateral for a class.
pub fn palw_seat_readiness_message_v1(network_domain: Hash64, bond: &[u8], class_id: &Hash64, span: u64, leaf_index: u32) -> Hash64 {
    let mut state = keyed64(PALW_SEAT_READINESS_V1_DOMAIN);
    state.update(network_domain.as_byte_slice());
    state.update(&(bond.len() as u64).to_le_bytes());
    state.update(bond);
    state.update(class_id.as_byte_slice());
    state.update(&span.to_le_bytes());
    state.update(&leaf_index.to_le_bytes());
    finish64(state)
}

/// **The shares the registry writes** (ADR-0135 Decision 5): each class's admission over every
/// class's, in permille, with the base class holding the remainder and never less than its floor.
/// A class with zero admission holds zero; the table always sums to 1,000.
pub fn palw_registry_shares_v1(
    admissions_milli: &[(Hash64, u64)],
    base_class: Hash64,
    base_floor_permille: u16,
) -> BTreeMap<Hash64, u16> {
    let floor = base_floor_permille.min(1_000) as u32;
    let room = 1_000u32 - floor;
    let others: Vec<(Hash64, u64)> = admissions_milli.iter().copied().filter(|(id, _)| *id != base_class).collect();
    let total: u128 = others.iter().map(|(_, a)| *a as u128).sum();
    let mut out = BTreeMap::new();
    let mut given = 0u32;
    for (id, a) in &others {
        let share = if total == 0 { 0 } else { ((*a as u128).saturating_mul(room as u128) / total).min(room as u128) as u32 };
        // A class that admits anything holds at least one permille: a target exists only for a share.
        let share = if *a > 0 && room > 0 { share.max(1) } else { share };
        given = given.saturating_add(share);
        out.insert(*id, share as u16);
    }
    out.insert(base_class, (1_000u32.saturating_sub(given)).max(floor) as u16);
    out
}

/// **What op 186 answers**: the registry as the tip state holds it.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct PalwModelRegistryReadV1 {
    pub tip_daa: u64,
    /// The fence's height, if scheduled, and whether it is in force at the tip.
    pub fence_daa: Option<u64>,
    pub active: bool,
    /// The registry governs from here (the fence plus one readiness age); before it rows open and
    /// proofs are taken but nothing is stepped or judged by evidence.
    pub grace_until_daa: u64,
    pub span_daa: u64,
    pub globals: Option<PalwRegistryGlobalsV1>,
    pub classes: Vec<PalwModelRegistryClassReadV1>,
    pub readiness: Vec<PalwSeatReadinessReadV1>,
    pub bonds: Vec<PalwRegistryBondReadV1>,
    /// Rowed classes by state: (active, active_limited, probation, prefetching, registered, held).
    pub counts: [u32; 6],
}

/// Why a row is where it is, from its last boundary's reading.
pub fn palw_lifecycle_reason_v1(row: &PalwModelLifecycleRowV1, is_base_class: bool, g: &PalwRegistryGlobalsV1) -> String {
    if is_base_class {
        return "base class: always active, never gated".to_string();
    }
    let seats = g.seat_count as u32;
    match row.state {
        PalwModelLifecycleV1::Registered => "no work derived from the graph (the VM boundary): never admits".to_string(),
        PalwModelLifecycleV1::Prefetching => {
            format!("ready {} < {} required (seats prove possession to be counted)", row.ready_seats, row.profile.required_ready_seats)
        }
        PalwModelLifecycleV1::Probation { probes_passed } => {
            format!("probing: {probes_passed}/{} finals passed, {} failed since entry", g.probation_claims, row.probes_failed)
        }
        PalwModelLifecycleV1::ActiveLimited { stable_epochs } => {
            if stable_epochs >= g.stable_epochs {
                format!(
                    "at a tenth: cap-saturated ({} ‰ of the escrow at the rate; not activatable above the ceiling)",
                    row.cap_utilization_permille
                )
            } else {
                format!("stable {stable_epochs}/{} spans at a tenth", g.stable_epochs)
            }
        }
        PalwModelLifecycleV1::Active => "admitting in full".to_string(),
        PalwModelLifecycleV1::Held => {
            if row.ready_seats < seats {
                format!(
                    "held: ready {} < {seats} for a panel (recovers through probation once {} are ready)",
                    row.ready_seats, row.profile.required_ready_seats
                )
            } else {
                format!("held: overloaded ({} ‰ utilization at {} in flight)", row.utilization_permille, row.inflight_claims)
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwModelRegistryClassReadV1 {
    pub class_id: Hash64,
    pub artifact_root: Hash64,
    pub is_base_class: bool,
    pub row: Option<PalwModelLifecycleRowV1>,
    /// Ready seats and claims in flight read NOW (the row keeps the last boundary's reading).
    pub ready_seats_now: u32,
    pub inflight_now: u32,
    pub share_permille: Option<u16>,
    /// Claims of the class voided as `NoCapablePanel` (the capacity's failure, counted).
    pub no_capable_panel_voids: u32,
    /// Why the row is where it is, from its last reading: `ready {n} < {seats} for a panel`,
    /// `overloaded`, `ready {n} < {required} required`, `probing {passed}/{needed}`, `stable
    /// {n}/{needed}`, `admitting`, `base class`, `no work (VM boundary)` — for an operator to tell
    /// a HELD by the rule from a HELD by a fault.
    pub reason: String,
}

/// A bond as the registry sees it now: whether it could count as a ready seat if it held a fresh
/// proof — the seat's own pre-check before it spends a fee on one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwRegistryBondReadV1 {
    pub bond: crate::palw_state_v2::PalwBondKeyV2,
    pub active: bool,
    pub above_floor: bool,
    pub free_collateral_sompi: u128,
    pub needed_collateral_sompi: u128,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwSeatReadinessReadV1 {
    pub bond: crate::palw_state_v2::PalwBondKeyV2,
    pub class_id: Hash64,
    pub row: PalwSeatReadinessRowV1,
    pub fresh: bool,
    /// Why the seat does not count as ready now, if it does not: `stale`, `bond inactive`,
    /// `below floor`, `collateral short`; empty while it counts.
    pub not_ready_reason: String,
}

/// Why a seat with a proof does not count as ready for a class now, or `None` while it does.
pub fn palw_seat_not_ready_reason_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    params: &crate::palw_state_v2::PalwStateParamsV2,
    bond_key: &crate::palw_state_v2::PalwBondKeyV2,
    row: &PalwSeatReadinessRowV1,
    now_daa: u64,
    fold: &PalwModelRegistryFoldV1,
) -> Option<&'static str> {
    let max_age_daa = (fold.globals.readiness_probe_max_age_spans as u64).saturating_mul(fold.span_daa.max(1));
    let floor = params.min_collateral_sompi();
    let needed = (floor as u128).saturating_mul(fold.globals.readiness_collateral_multiple as u128);
    let Some(bond) = state.bond(bond_key) else { return Some("bond missing") };
    if !matches!(bond.status, crate::palw_state_v2::PalwBondStatusV2::Active) {
        return Some("bond inactive");
    }
    if !crate::palw_state_v2::palw_bond_may_take_work_v2(bond, floor) {
        return Some("below floor");
    }
    if now_daa.saturating_sub(row.proved_daa) > max_age_daa {
        return Some("stale");
    }
    let held = state.reserved_exposure(bond_key).saturating_add(state.registration_exposure(bond_key));
    let free = (bond.collateral as u128).saturating_sub(bond.slashed as u128).saturating_sub(held);
    if free < needed {
        return Some("collateral short");
    }
    None
}

/// The seats ready for a class now (ADR-0135 Decision 4), as the fold counts them: active, above
/// the floor, with a possession proof no older than the readiness age, and free collateral for
/// the readiness multiple of the network's floor.
pub fn palw_model_registry_ready_seats_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    params: &crate::palw_state_v2::PalwStateParamsV2,
    class_id: &Hash64,
    now_daa: u64,
    fold: &PalwModelRegistryFoldV1,
) -> u32 {
    let max_age_daa = (fold.globals.readiness_probe_max_age_spans as u64).saturating_mul(fold.span_daa.max(1));
    let floor = params.min_collateral_sompi();
    let needed = (floor as u128).saturating_mul(fold.globals.readiness_collateral_multiple as u128);
    let mut ready = 0u32;
    for (bond_key, bond) in state.bonds_iter() {
        if !matches!(bond.status, crate::palw_state_v2::PalwBondStatusV2::Active)
            || !crate::palw_state_v2::palw_bond_may_take_work_v2(bond, floor)
        {
            continue;
        }
        let Some(row) = state.seat_readiness(bond_key, class_id) else { continue };
        if now_daa.saturating_sub(row.proved_daa) > max_age_daa {
            continue;
        }
        let held = state.reserved_exposure(bond_key).saturating_add(state.registration_exposure(bond_key));
        let free = (bond.collateral as u128).saturating_sub(bond.slashed as u128).saturating_sub(held);
        if free < needed {
            continue;
        }
        ready = ready.saturating_add(1);
    }
    ready
}

/// Attempt claims of a class still in flight (accepted and not terminal).
pub fn palw_model_registry_inflight_v1(state: &crate::palw_state_v2::PalwChainStateV2, class_id: &Hash64) -> u32 {
    state
        .claims_iter()
        .filter(|(_, claim)| {
            claim.class_id == *class_id
                && matches!(claim.source, crate::palw_state_v2::PalwClaimSourceV2::Attempt)
                && !claim.phase.is_terminal()
        })
        .count()
        .min(u32::MAX as usize) as u32
}

/// The registry read of a tip state.
pub fn palw_model_registry_read_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    params: &crate::palw_state_v2::PalwStateParamsV2,
    tip_daa: u64,
    fence_daa: Option<u64>,
    fold: Option<&PalwModelRegistryFoldV1>,
) -> PalwModelRegistryReadV1 {
    let base = params.base_class_id();
    let classes = state
        .classes_iter()
        .map(|(class_id, record)| PalwModelRegistryClassReadV1 {
            class_id: *class_id,
            artifact_root: record.artifact_root,
            is_base_class: *class_id == base,
            row: state.model_lifecycle(class_id).cloned(),
            ready_seats_now: fold.map(|f| palw_model_registry_ready_seats_v1(state, params, class_id, tip_daa, f)).unwrap_or(0),
            inflight_now: palw_model_registry_inflight_v1(state, class_id),
            share_permille: state.class_share_permille(class_id),
            reason: match (state.model_lifecycle(class_id), fold) {
                (Some(row), Some(f)) => palw_lifecycle_reason_v1(row, *class_id == base, &f.globals),
                (Some(_), None) => "the registry is not in force".to_string(),
                (None, _) => "no row (registered before the fence without a carriage): never gated".to_string(),
            },
            no_capable_panel_voids: state
                .claims_iter()
                .filter(|(_, c)| {
                    c.class_id == *class_id
                        && matches!(
                            c.phase,
                            crate::palw_state_v2::PalwClaimPhaseV2::Voided {
                                reason: crate::palw_state_v2::PalwVoidReasonV2::NoCapablePanel,
                                ..
                            }
                        )
                })
                .count()
                .min(u32::MAX as usize) as u32,
        })
        .collect();
    let max_age_daa = fold.map(|f| (f.globals.readiness_probe_max_age_spans as u64).saturating_mul(f.span_daa.max(1)));
    let readiness = state
        .seat_readiness_iter()
        .map(|((bond, class_id), row)| PalwSeatReadinessReadV1 {
            bond: *bond,
            class_id: *class_id,
            row: *row,
            fresh: max_age_daa.is_some_and(|age| tip_daa.saturating_sub(row.proved_daa) <= age),
            not_ready_reason: fold
                .and_then(|f| palw_seat_not_ready_reason_v1(state, params, bond, row, tip_daa, f))
                .unwrap_or("")
                .to_string(),
        })
        .collect();
    let bonds = fold
        .map(|f| {
            let floor = params.min_collateral_sompi();
            let needed = (floor as u128).saturating_mul(f.globals.readiness_collateral_multiple as u128);
            state
                .bonds_iter()
                .map(|(key, bond)| {
                    let held = state.reserved_exposure(key).saturating_add(state.registration_exposure(key));
                    PalwRegistryBondReadV1 {
                        bond: *key,
                        active: matches!(bond.status, crate::palw_state_v2::PalwBondStatusV2::Active),
                        above_floor: crate::palw_state_v2::palw_bond_may_take_work_v2(bond, floor),
                        free_collateral_sompi: (bond.collateral as u128).saturating_sub(bond.slashed as u128).saturating_sub(held),
                        needed_collateral_sompi: needed,
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let mut counts = [0u32; 6];
    for (_, row) in state.model_lifecycles_iter() {
        let slot = match row.state {
            PalwModelLifecycleV1::Active => 0,
            PalwModelLifecycleV1::ActiveLimited { .. } => 1,
            PalwModelLifecycleV1::Probation { .. } => 2,
            PalwModelLifecycleV1::Prefetching => 3,
            PalwModelLifecycleV1::Registered => 4,
            PalwModelLifecycleV1::Held => 5,
        };
        counts[slot] += 1;
    }
    PalwModelRegistryReadV1 {
        tip_daa,
        fence_daa,
        active: fold.is_some(),
        grace_until_daa: fold.map(|f| f.grace_until_daa).unwrap_or(0),
        span_daa: fold.map(|f| f.span_daa).unwrap_or(0),
        globals: fold.map(|f| f.globals),
        classes,
        readiness,
        bonds,
        counts,
    }
}

/// **When a seat's node should submit a fresh possession proof** (the node's duty, ADR-0135 §7):
/// when the chain holds no proof of this bond for the class, or the one it holds is older than
/// half the readiness age (so a proof lands before the old one goes stale), and never twice in
/// one span. Pure, so the node's cadence is testable: a restart re-reads the chain's row and does
/// not re-send what is fresh.
pub fn palw_readiness_duty_due_v1(
    row: Option<&PalwSeatReadinessRowV1>,
    now_daa: u64,
    span_now: u64,
    last_submitted_span: Option<u64>,
    span_daa: u64,
    g: &PalwRegistryGlobalsV1,
) -> bool {
    if last_submitted_span == Some(span_now) {
        return false;
    }
    let age_daa = (g.readiness_probe_max_age_spans as u64).saturating_mul(span_daa.max(1));
    match row {
        None => true,
        Some(row) => now_daa.saturating_sub(row.proved_daa) > age_daa / 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_execution_lane_v1::{PalwExecFinalV1, palw_execution_schedule_snapshot_v1};
    use crate::palw_state_v2::PalwBondKeyV2;
    use crate::tx::{TransactionId, TransactionOutpoint};

    const G: PalwRegistryGlobalsV1 = PALW_REGISTRY_GLOBALS_V1;
    fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }
    fn manifest(bytes: u64, prefill: u32) -> PalwModelManifestV1 {
        PalwModelManifestV1 {
            graph_ir_root: h(1),
            artifact_root: h(2),
            artifact_bytes: bytes,
            canonical_prefill_tokens: prefill,
            canonical_decode_tokens: 1,
            quantization_format: 1,
            runtime_version: 3,
        }
    }
    fn work(verification_ccu: u128, economic: u128, bytes: u64) -> PalwModelWorkV1 {
        PalwModelWorkV1 {
            verification_ccu,
            economic_ccu_per_claim: economic,
            artifact_bytes: bytes,
            working_set_bytes: bytes,
            ops_supported: true,
        }
    }
    /// The live classes as ADR-0131/0132 measured them, and a Kimi-class stand-in.
    fn dense() -> PalwModelWorkV1 {
        work(83_102_171_136, 166_204_342_272, 800 << 20)
    }
    fn hybrid() -> PalwModelWorkV1 {
        work(18_055_200_736, 36_110_401_472, 24 << 30)
    }
    fn kimi() -> PalwModelWorkV1 {
        work(1_000_000_000_000, 2_000_000_000_000, 300 << 30)
    }

    /// **The profile is a function of the manifest's work and the globals, and of nothing a human
    /// measured or a registrant stated.** Two nodes derive one profile; the dense tier is one span
    /// of verification (35 ms of the reference's 600 s) plus the receipt allowance, the hybrid the
    /// same, the Kimi stand-in `⌈2 × 1 T / 2.4 T⌉ = 1` + 1 spans of verification but two spans of
    /// prefetch for 300 GiB, seven ready seats, a bond of 2,000 MSK, and an admission of 8.3 claims
    /// a span under a budget of ten dense-tier claims' attempted compute.
    #[test]
    fn adr0135_the_profile_is_derived_from_work_alone() {
        let a = palw_derive_profile_v1(&dense(), &G);
        let b = palw_derive_profile_v1(&dense(), &G);
        assert_eq!(a, b, "deterministic");
        assert_eq!((a.verification_window_spans, a.artifact_prefetch_spans, a.required_ready_seats), (2, 1, 7));
        assert_eq!(a.registration_bond_sompi, 2 * 1_000 * 100_000_000);
        assert_eq!(a.admission_claims_per_span_milli, 10_000, "the budget is ten of its own claims a span");
        let k = palw_derive_profile_v1(&kimi(), &G);
        assert_eq!(k.verification_window_spans, 2, "2 × 1 T MAC-eq is under one reference span (2.4 T), plus the allowance");
        assert_eq!(k.artifact_prefetch_spans, 2, "2 × 300 GiB at 600 GB a span");
        assert_eq!(k.required_ready_seats, 7);
        assert!(k.max_inflight_claims < a.max_inflight_claims);
        assert_eq!(k.admission_claims_per_span_milli, 831, "the budget affords 0.83 Kimi claims a span: rare and dear");
        // A registrant's declaration changes nothing: the manifest carries no window and no rate,
        // and a heavier graph gets a wider window from the same code.
        let heavier = work(10_000_000_000_000, 20_000_000_000_000, 300 << 30);
        assert!(palw_derive_profile_v1(&heavier, &G).verification_window_spans > k.verification_window_spans);
        assert_eq!(
            palw_derive_profile_v1(&heavier, &G).verification_window_spans,
            9 + 1,
            "2 × 10 T over 2.4 T a span: nine, plus the allowance"
        );
        assert!(
            palw_derive_profile_v1(&heavier, &G).required_ready_seats > 7,
            "one such claim a span at 70 % needs more than seven seats"
        );
    }

    /// **The manifest is judged before it is a class, and the VM boundary is one verdict.**
    #[test]
    fn adr0135_a_manifest_is_judged_and_the_vm_boundary_is_a_verdict() {
        assert_eq!(palw_manifest_verdict_v1(&manifest(1, 7), &dense()), PalwManifestVerdictV1::Valid);
        let alien = PalwModelWorkV1 { ops_supported: false, ..dense() };
        assert_eq!(
            palw_manifest_verdict_v1(&manifest(1, 7), &alien),
            PalwManifestVerdictV1::UnsupportedOp,
            "a new opcode is a VM upgrade, not a registration"
        );
        assert_eq!(palw_manifest_verdict_v1(&manifest(0, 7), &dense()), PalwManifestVerdictV1::EmptyArtifact);
        assert_eq!(palw_manifest_verdict_v1(&manifest(1, 0), &dense()), PalwManifestVerdictV1::EmptyJob);
        assert_eq!(palw_manifest_verdict_v1(&manifest(1, 7), &work(0, 0, 1)), PalwManifestVerdictV1::ZeroWork);
    }

    /// **Readiness is evidence, not a declaration.** A seat that only says "I have it" is not
    /// ready; one with the root, the chunks, participation, collateral and a fresh probe is; the
    /// probe goes stale; the collateral must cover three exposures.
    #[test]
    fn adr0135_readiness_is_evidence_not_a_declaration() {
        let exposure = 5 * 6_630_544u128;
        let declared_only = PalwReadinessEvidenceV1 { artifact_root_matches: true, ..Default::default() };
        assert!(!declared_only.is_ready(exposure, 100, &G));
        let ready = PalwReadinessEvidenceV1 {
            artifact_root_matches: true,
            all_chunks_held: true,
            participation_ok: true,
            free_collateral_sompi: exposure * 3,
            last_probe_ok_span: Some(90),
        };
        assert!(ready.is_ready(exposure, 100, &G));
        assert!(
            !PalwReadinessEvidenceV1 { participation_ok: false, ..ready }.is_ready(exposure, 100, &G),
            "an IBD-held node is not a seat"
        );
        assert!(!PalwReadinessEvidenceV1 { free_collateral_sompi: exposure * 3 - 1, ..ready }.is_ready(exposure, 100, &G));
        assert!(!ready.is_ready(exposure, 90 + 31, &G), "a probe older than thirty spans has gone stale");
        assert!(
            !PalwReadinessEvidenceV1 { all_chunks_held: false, ..ready }.is_ready(exposure, 100, &G),
            "a partial artifact is not held"
        );
    }

    /// **The lifecycle walks on facts, and a held class stops only itself.** A registered class
    /// prefetches once its manifest is valid, enters probation when seven seats prove ready,
    /// activates limited after ten passed probes with none failed, activates after three stable
    /// epochs, and is held the span its panel cannot be drawn — while a second class in the same
    /// network, on its own facts, stays `Active`, and the lane schedules exactly the classes with a
    /// `Final`. Recovery returns through probation.
    #[test]
    fn adr0135_the_lifecycle_walks_on_facts_and_a_held_class_stops_only_itself() {
        use PalwModelLifecycleV1::*;
        let k = palw_derive_profile_v1(&kimi(), &G);
        let q = palw_derive_profile_v1(&dense(), &G);
        let calm = |ready: u32| PalwLifecycleObservationV1 {
            manifest: PalwManifestVerdictV1Flag::Valid,
            ready_seats: ready,
            probes_passed_this_span: 0,
            probes_failed_this_span: 0,
            utilization_permille: 300,
            collateral_ok: true,
            cap_ok: true,
            span_stable: true,
        };
        let mut s = Registered;
        s = palw_lifecycle_step_v1(s, &PalwLifecycleObservationV1 { manifest: PalwManifestVerdictV1Flag::Invalid, ..calm(0) }, &k, &G);
        assert_eq!(s, Registered, "an invalid manifest never leaves registration");
        s = palw_lifecycle_step_v1(s, &calm(0), &k, &G);
        assert_eq!(s, Prefetching);
        s = palw_lifecycle_step_v1(s, &calm(6), &k, &G);
        assert_eq!(s, Prefetching, "six ready seats are not the seven the profile needs");
        s = palw_lifecycle_step_v1(s, &calm(7), &k, &G);
        assert_eq!(s, Probation { probes_passed: 0 });
        assert!(
            !s.admits_claims() || s.admission_permille() == 50,
            "probation admits a twentieth: the claims that probe it must exist"
        );
        s = palw_lifecycle_step_v1(s, &PalwLifecycleObservationV1 { probes_passed_this_span: 4, ..calm(7) }, &k, &G);
        s = palw_lifecycle_step_v1(
            s,
            &PalwLifecycleObservationV1 { probes_passed_this_span: 4, probes_failed_this_span: 1, ..calm(7) },
            &k,
            &G,
        );
        assert_eq!(s, Probation { probes_passed: 0 }, "a failed probe restarts probation");
        for _ in 0..3 {
            s = palw_lifecycle_step_v1(s, &PalwLifecycleObservationV1 { probes_passed_this_span: 4, ..calm(7) }, &k, &G);
        }
        assert_eq!(s, ActiveLimited { stable_epochs: 0 });
        assert_eq!(s.admission_permille(), 100);
        for _ in 0..3 {
            s = palw_lifecycle_step_v1(s, &calm(7), &k, &G);
        }
        assert_eq!(s, Active);
        assert_eq!(s.admission_permille(), 1_000);
        // Two operators go: four ready seats cannot draw a panel of five — Kimi is held.
        let held = palw_lifecycle_step_v1(s, &calm(4), &k, &G);
        assert_eq!(held, Held);
        assert!(!held.admits_claims());
        // …and the dense tier, on its own facts, is untouched.
        let mut qs = Active;
        qs = palw_lifecycle_step_v1(qs, &calm(7), &q, &G);
        assert_eq!(qs, Active, "another class's outage is not this class's");
        // The lane: only the class with Finals holds permits.
        let finals = vec![PalwExecFinalV1 {
            domain: h(1),
            bond: PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(1), 0)),
            operator_id: h(11),
            claim_id: h(100),
            execution_root: h(7),
            credit: 1,
        }];
        let snapshot = palw_execution_schedule_snapshot_v1(3, &finals);
        assert!(snapshot.domains.iter().any(|d| d.domain == h(1)) && snapshot.domains.iter().all(|d| d.domain != h(2)));
        // Recovery: seats return → probation again, never straight to Active.
        let back = palw_lifecycle_step_v1(held, &calm(7), &k, &G);
        assert_eq!(back, Probation { probes_passed: 0 });
        // Overload holds too, even with seats.
        assert_eq!(
            palw_lifecycle_step_v1(Active, &PalwLifecycleObservationV1 { utilization_permille: 1_000, ..calm(9) }, &k, &G),
            Held
        );
    }

    /// **No share is set by anyone.** The single lottery's targets follow admission: the dense
    /// tier and the hybrid at the live compute split the draws by their admitted claims a span,
    /// a Kimi-class registered beside them takes the slice its rarity affords, and a class whose
    /// admission is zero holds nothing; the budget is what stays constant.
    #[test]
    fn adr0135_shares_follow_admission_and_no_one_sets_them() {
        let (d, hy, k) =
            (palw_derive_profile_v1(&dense(), &G), palw_derive_profile_v1(&hybrid(), &G), palw_derive_profile_v1(&kimi(), &G));
        assert!(hy.admission_claims_per_span_milli > d.admission_claims_per_span_milli, "the lighter claim is admitted more often");
        let two = palw_class_shares_from_admission_v1(&[
            (h(1), d.admission_claims_per_span_milli),
            (h(2), hy.admission_claims_per_span_milli),
        ]);
        let three = palw_class_shares_from_admission_v1(&[
            (h(1), d.admission_claims_per_span_milli),
            (h(2), hy.admission_claims_per_span_milli),
            (h(3), k.admission_claims_per_span_milli),
        ]);
        let sum = |v: &[(Hash64, u16)]| v.iter().map(|(_, s)| *s as u32).sum::<u32>();
        assert!((998..=1_000).contains(&sum(&two)) && (998..=1_000).contains(&sum(&three)));
        assert!(three[2].1 < three[0].1 && three[2].1 > 0, "Kimi's slice: rare, not nothing: {:?}", three);
        assert!(three[0].1 < two[0].1, "a new class takes its slice from everyone in proportion");
        let with_zero = palw_class_shares_from_admission_v1(&[(h(1), 1_000), (h(2), 0)]);
        assert_eq!(with_zero, vec![(h(1), 1_000), (h(2), 0)]);
        // The budget in compute is what is conserved: Σ admission × economic compute ≈ budget.
        let spent: u128 = [(&dense(), d), (&hybrid(), hy), (&kimi(), k)]
            .iter()
            .map(|(w, p)| w.economic_ccu_per_claim.saturating_mul(p.admission_claims_per_span_milli as u128) / 1_000)
            .sum();
        assert!(spent <= 3 * G.budget_ccu_per_span, "each class spends at most the budget; a global cap then scales them");
    }

    #[test]
    fn adr0135_the_node_proves_when_the_chain_has_no_fresh_proof_and_never_twice_a_span() {
        let g = PALW_REGISTRY_GLOBALS_V1;
        let span = 5;
        assert!(palw_readiness_duty_due_v1(None, 1_000, 200, None, span, &g), "no proof on the chain: due");
        assert!(!palw_readiness_duty_due_v1(None, 1_000, 200, Some(200), span, &g), "already sent this span");
        let fresh = PalwSeatReadinessRowV1 { proved_daa: 990, proved_span: 198, leaf_index: 3 };
        assert!(!palw_readiness_duty_due_v1(Some(&fresh), 1_000, 200, None, span, &g), "a fresh proof is not repeated");
        let half = PalwSeatReadinessRowV1 {
            proved_daa: 1_000 - (g.readiness_probe_max_age_spans as u64 * span) / 2 - 1,
            proved_span: 0,
            leaf_index: 3,
        };
        assert!(
            palw_readiness_duty_due_v1(Some(&half), 1_000, 200, None, span, &g),
            "past half the age: renewed before it goes stale"
        );
        assert!(!palw_readiness_duty_due_v1(Some(&half), 1_000, 200, Some(200), span, &g));
    }

    /// **ADR-0132 Upgrade C / ADR-0133 Fence 3: a cap-saturated class is not activatable.** The
    /// stable count keeps running; the full share does not come, and an active class over the
    /// ceiling falls back to a tenth. Probation still exits to a tenth: the cap gates the full
    /// share only.
    #[test]
    fn adr0132_a_cap_saturated_class_is_not_activated_and_an_active_one_falls_to_a_tenth() {
        use PalwModelLifecycleV1::*;
        let k = palw_derive_profile_v1(&kimi(), &G);
        let obs = |cap_ok: bool| PalwLifecycleObservationV1 {
            manifest: PalwManifestVerdictV1Flag::Valid,
            ready_seats: 7,
            probes_passed_this_span: 0,
            probes_failed_this_span: 0,
            utilization_permille: 300,
            collateral_ok: true,
            cap_ok,
            span_stable: true,
        };
        assert_eq!(
            palw_lifecycle_step_v1(ActiveLimited { stable_epochs: 2 }, &obs(true), &k, &G),
            Active,
            "three stable spans activate"
        );
        assert_eq!(
            palw_lifecycle_step_v1(ActiveLimited { stable_epochs: 2 }, &obs(false), &k, &G),
            ActiveLimited { stable_epochs: 3 },
            "…unless the class is cap-saturated: it keeps counting and stays at a tenth"
        );
        assert_eq!(
            palw_lifecycle_step_v1(ActiveLimited { stable_epochs: 9 }, &obs(false), &k, &G),
            ActiveLimited { stable_epochs: 10 }
        );
        assert_eq!(
            palw_lifecycle_step_v1(Active, &obs(false), &k, &G),
            ActiveLimited { stable_epochs: 0 },
            "an active class over the ceiling falls back to a tenth"
        );
        assert_eq!(palw_lifecycle_step_v1(Active, &obs(true), &k, &G), Active);
        assert_eq!(
            palw_lifecycle_step_v1(Probation { probes_passed: 10 }, &obs(false), &k, &G),
            ActiveLimited { stable_epochs: 0 },
            "probation still exits to a tenth: the cap gates the full share only"
        );
    }
}
