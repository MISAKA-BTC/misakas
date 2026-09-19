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

/// **How many spans late a possession proof may land** (ADR-0135 §7, found by the devnet drill):
/// a proof names the span it was made for; the carrier that brings it waits in mempools and for a
/// block, and on a devnet with two-DAA spans that wait was several spans. A proof up to this many
/// spans old is taken, and the row it writes is dated at the NAMED span's first DAA — so a late
/// proof is exactly as fresh as when it was made, a replayed one renews nothing, and the readiness
/// age (thirty spans) bounds the rest.
pub const PALW_READINESS_LANDING_SPANS_V1: u64 = 8;

/// The same allowance in DAA, for a network whose spans are short: a carrier's wait is blocks and
/// minutes, not spans, and on a devnet with two-DAA spans eight spans were sixteen minutes — a
/// chain of eight receipts ahead of the proof took longer. The allowance is the larger of the two
/// (`max(8 spans, 40 DAA / span)`): on testnet-11's five-DAA spans it is the eight spans, on the
/// devnet twenty.
pub const PALW_READINESS_LANDING_DAA_V1: u64 = 40;

/// How many spans late a proof may land at `span_daa` DAA a span.
pub fn palw_readiness_landing_spans_v1(span_daa: u64) -> u64 {
    PALW_READINESS_LANDING_SPANS_V1.max(PALW_READINESS_LANDING_DAA_V1 / span_daa.max(1))
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
    /// **Registered is not eligible** (ADR-0145 §7, the 2026-09-19 audit's F3). A class a
    /// registrant bought exists, may be inspected, benchmarked and run in shadow — and earns no
    /// sompi and no unit of fork-choice weight — until the chain has seen a seat the registrant
    /// does NOT hold ready to verify it. Only then does it enter the ordinary walk at
    /// `Prefetching`. Written only past `Params::palw_admission_independence`; below the fence
    /// nothing constructs it and every existing row keeps the state it was written with.
    ///
    /// **It is last in this enum although it is first in the lifecycle**, and that is deliberate:
    /// the enum is borsh, its discriminants are chain bytes, and inserting a variant ahead of
    /// `Registered` would renumber every row already written — the exact failure that made a main
    /// build unable to sync testnet-11 from genesis on 2026-09-10. The order a reader wants lives
    /// in `palw_lifecycle_step_v1` and in this doc, never in the tag.
    Candidate,
}

impl PalwModelLifecycleV1 {
    /// Whether the class accepts new claims at all.
    ///
    /// `Candidate` is absent from this list for the reason it exists: a registration is existence,
    /// not eligibility. The list is positive — states that DO admit — so a state added later
    /// admits nothing until someone writes it here on purpose.
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
    /// **ADR-0133 §11.3 (fail-closed):** the class's derived verification window
    /// (`verification_window_spans × span_daa`) fits inside the network's receipt deadline
    /// (`window_receipt`). A class that needs longer to replay than a claim is given to be replayed
    /// in would void every claim it accepted; it is HELD, never ACTIVE, until a class-specific
    /// receipt deadline (its own fence, not built) or a wider global window admits it.
    pub window_fits_receipt: bool,
    /// Whether this span ran at or under the target utilization with no held claim.
    pub span_stable: bool,
    /// **ADR-0147: the class's admission jury sat at this boundary and a majority of it holds the
    /// class** (past `Params::palw_admission_independence`).
    ///
    /// The jury is `seat_count` operators drawn from the NETWORK's base-class population — not
    /// from the class's own — by a lottery whose seed is the execution lane's anchor of the span
    /// before, over bonds registered before that span began, on the audit schedule
    /// ([`palw_admission_audit_due_v1`]). A juror counts when one of its bonds is READY for the
    /// class by the registry's own five-clause predicate. So the registrant passes alone only by
    /// holding a majority of a jury the network drew, which it does with the probability that its
    /// share of the network's operator lottery wins a majority of `seat_count` draws — once per
    /// audit, not once per span.
    ///
    /// Read ONLY by the `Candidate` arm — a state nothing writes below the fence — so every other
    /// transition is the one it was before this field existed, and a caller that leaves it `false`
    /// (the `Default`) can never turn an admitted class back. `false` for a genesis class by
    /// construction: the fold never puts one in `Candidate`.
    pub admission_jury_seated: bool,
}

/// **ADR-0147: how often a `Candidate` class meets an admission jury**, in execution spans — one
/// epoch's worth, at least one span.
///
/// The jury is a lottery, and a lottery that may be re-run every span is passed by waiting: at a
/// five-DAA span a registrant whose share wins the jury one time in a thousand would be admitted
/// in under an hour and a half. The rate the chain CAN bound is how often the draw is taken, and
/// the epoch is the unit this chain already budgets everything else in (ADR-0039 D5). Stateless on
/// purpose: an audit falls on the spans whose index is a multiple of the period, so no row records
/// the last one and none can be written or rewound to run another.
pub fn palw_admission_audit_period_spans_v1(epoch_length: u64, span_daa: u64) -> u64 {
    (epoch_length / span_daa.max(1)).max(1)
}

/// Whether the span a boundary opens is an audit span ([`palw_admission_audit_period_spans_v1`]).
/// Span zero never is: the jury's seed is the anchor of the span before, and there is none.
pub fn palw_admission_audit_due_v1(span_now: u64, period_spans: u64) -> bool {
    span_now > 0 && span_now.is_multiple_of(period_spans.max(1))
}

/// A strict majority of the jury: `seats / 2 + 1`, which is the panel's own quorum shape
/// (`2·quorum > seat_count`) at its smallest.
pub fn palw_admission_jury_quorum_v1(seats: u16) -> u16 {
    seats / 2 + 1
}

pub const PALW_ADMISSION_JURY_SEED_DOMAIN: &[u8] = b"misaka-palw/admission-jury/seed/v1";

/// **ADR-0147: the admission jury's seed** — the class, the audit span, and the execution lane's
/// seed anchor of the span before (its chain block and its attempt's execution commitment).
///
/// The anchor is ADR-0130's randomness, and the reason it is used here rather than a block hash:
/// re-rolling it costs a winning inference, where a boundary block's own hash can be re-rolled by
/// its producer for the price of a header. The population the jury is drawn from is cut before the
/// anchor's span began, so no bond in it was registered by a party that had seen the seed.
pub fn palw_admission_jury_seed_v1(class_id: &Hash64, span: u64, anchor_block: &Hash64, execution_key: &Hash64) -> Hash64 {
    let mut state = keyed64(PALW_ADMISSION_JURY_SEED_DOMAIN);
    state.update(class_id.as_byte_slice());
    state.update(&span.to_le_bytes());
    state.update(anchor_block.as_byte_slice());
    state.update(execution_key.as_byte_slice());
    finish64(state)
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
    // ADR-0133 §11.3: a class whose replay does not fit the receipt deadline is treated as
    // overloaded for ever — it admits nothing, and it re-enters Probation only once it fits.
    let overloaded = obs.utilization_permille >= 1_000 || !obs.window_fits_receipt;
    match state {
        // **The one transition a registrant cannot make alone** (ADR-0147, audit F3). A class
        // leaves `Candidate` when a jury the NETWORK drew — `seat_count` operators of the
        // liveness floor's population, not of the class's — finds a majority of itself holding
        // the class. Not when the registrant says it is ready, and not when the registrant's own
        // seats prove possession: the class's own population is a set the registrant fills first,
        // and for a model nobody else runs it fills it entirely. Until then the class exists and
        // admits nothing.
        //
        // It rejoins the ordinary walk exactly where a registration used to start, so nothing
        // downstream of `Prefetching` learns a new state: the manifest verdict still decides
        // whether there is any work to prefetch for, and `Registered` still means "no work derived
        // from the graph", which independence cannot cure.
        Candidate => {
            if !obs.admission_jury_seated {
                Candidate
            } else if obs.manifest == PalwManifestVerdictV1Flag::Valid {
                Prefetching
            } else {
                Registered
            }
        }
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

/// **The shipped classes' work, from the typed catalog every node compiles** — the floor, the
/// Qwen3.6 hybrid, the Qwen2.5 A16 graph-v5 row and the Qwen3.8-27B row, each with the canonical
/// job its registration prices — for the genesis classes no bundle registration describes (a genesis
/// class carries no admission carriage; its profile is the catalog's, of which the bundle holds only
/// a root). A function of the binary, so every node running it derives the same works.
pub fn palw_rc_typed_class_works_v1() -> BTreeMap<Hash64, PalwModelWorkV1> {
    use crate::palw_base0_profile::{PALW_RC_BASE0_CANONICAL, PALW_RC_BASE0_GEOMETRY, base0_profile_v1, rc_job_context};
    use crate::palw_context_ladder::palw_a16_context_row_profile_v5;
    use crate::palw_qwen25_profile::{QWEN25_A16_GRAPH_V5_N_CTX, qwen25_a16_graph_v5_canonical_v1};
    use crate::palw_qwen36_profile::qwen36_geometry_artifact_eps;
    use crate::palw_qwen36_profile::{QWEN36_35B_A3B, QWEN36_RC_CANONICAL, QWEN38_27B, qwen36_profile_v2};
    let mut rows: Vec<(crate::palw_step::PalwShapeProfileV3, (u32, u32))> = Vec::with_capacity(4);
    if let Ok(floor) = base0_profile_v1(PALW_RC_BASE0_GEOMETRY) {
        rows.push((floor, PALW_RC_BASE0_CANONICAL));
    }
    if let Ok(hybrid) = qwen36_profile_v2(qwen36_geometry_artifact_eps(QWEN36_35B_A3B)) {
        rows.push((hybrid, QWEN36_RC_CANONICAL));
    }
    if let Ok(dense) = palw_a16_context_row_profile_v5(QWEN25_A16_GRAPH_V5_N_CTX) {
        rows.push((dense, qwen25_a16_graph_v5_canonical_v1()));
    }
    if let Ok(unit) = qwen36_profile_v2(qwen36_geometry_artifact_eps(QWEN38_27B)) {
        rows.push((unit, QWEN36_RC_CANONICAL));
    }
    rows.into_iter()
        .filter_map(|(profile, (prefill, decode))| {
            let canonical = rc_job_context(&profile, prefill, decode);
            palw_model_work_from_carriage_v1(&profile, &canonical).map(|work| (profile.shape_profile_id(), work))
        })
        .collect()
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
    /// **The share the registry priced the class's target for** (ADR-0135 §7, the devnet drill's
    /// seventh finding), `0` until the registry has seated it. A registration copies the floor's
    /// target (op 180's terms), a price for the floor's draw rate and not the class's; ADR-0076
    /// says a class being seated is a class being priced, and the activation edge already prices a
    /// weightless class from its share and its counted work. The registry's admission is the same
    /// event: at the first governed boundary a class holds a nonzero share in an admitted state, its
    /// target is `attempt_target_seed_v1(share, pwu)` and this records the share. Once — from there
    /// the class DAA owns the target, and a share the registry moves later is measured by the
    /// retarget against the history the class then has. The floor is never priced here.
    pub priced_share_permille: u16,
}

/// **A seat's readiness for a class**: the last possession proof this bond opened for the class
/// (ADR-0135 Decision 4). Fresh while `proved_daa` is within the readiness age; the probe half of
/// readiness is the seat's last `Valid` receipt on the class, read off the claims.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwSeatReadinessRowV1 {
    pub proved_daa: u64,
    pub proved_span: u64,
    pub leaf_index: u32,
    /// **Which evidence wrote this row** (H-6, ADR-0133 §11.2): `1` for a V1 proof — one leaf of a
    /// contiguous eight-leaf window — and `2` for a `SeatReadinessProvedV2` multiproof over leaves
    /// drawn from the whole artifact. Past `Params::palw_readiness_v2` only a V2 row counts a seat
    /// ready, so a V1 row ages out instead of standing in for possession it never showed.
    pub proof_version: u8,
    /// How many leaves the proof opened (1 for V1).
    pub chunks: u32,
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

/// **ADR-0133 §11.2 / H-6: the leaves a V2 possession proof opens.** Sixteen, drawn independently
/// from the WHOLE artifact rather than from one contiguous window of eight — a seat holding a
/// fraction `f` of the artifact answers a challenge with probability `f¹⁶`, so possession is what is
/// shown rather than reach. Capped at the inventory's own size for a small artifact.
pub const PALW_READINESS_V2_CHUNKS_V1: u32 = 16;
/// **How fresh a V2 row must be to count a seat ready**, in execution spans. The challenge already
/// rotates every span (the seed carries it); this is what makes the rotation bite — a proof stands
/// for eight spans, not the thirty a V1 row was given.
pub const PALW_READINESS_V2_MAX_AGE_SPANS_V1: u32 = 8;
/// The bytes a V2 proof's opened operands may carry in total, before the object's own cap.
pub const PALW_READINESS_V2_OPERAND_MAX_BYTES_V1: usize = 1 << 20;
pub const PALW_SEAT_READINESS_V2_DOMAIN: &[u8] = b"misaka-palw/seat-readiness-v2/message/v1";
pub const PALW_SEAT_READINESS_V2_CHALLENGE_DOMAIN: &[u8] = b"misaka-palw/seat-readiness-v2/challenge/v1";
pub const PALW_SEAT_READINESS_V2_MLDSA87_CONTEXT: &[u8] = b"misaka-palw/seat-readiness-v2/mldsa87/v1";

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
/// **ADR-0133 §11.2: the V2 challenge — `k` leaves of the whole artifact, for this (class, bond,
/// span).** Independent draws from the full range, de-duplicated by advancing the counter, returned
/// in ascending order. A function of the chain alone: the seed is
/// [`palw_readiness_v2_challenge_seed_v1`], so the prover, every validator and the court draw the
/// same set, and no seat learns its leaves before the span it must prove them in.
pub fn palw_readiness_v2_leaves_v1(seed: &Hash64, leaf_count: u32) -> Vec<u32> {
    if leaf_count == 0 {
        return Vec::new();
    }
    let want = PALW_READINESS_V2_CHUNKS_V1.min(leaf_count) as usize;
    let mut picked: Vec<u32> = Vec::with_capacity(want);
    // A bounded walk: every draw either adds a leaf or collides, and a collision costs one counter
    // step. `want ≤ leaf_count`, so the walk terminates well inside the cap.
    let cap = (want as u64).saturating_mul(64).saturating_add(256);
    let mut counter = 0u64;
    while picked.len() < want && counter < cap {
        let mut state = keyed64(PALW_SEAT_READINESS_V2_CHALLENGE_DOMAIN);
        state.update(seed.as_byte_slice());
        state.update(&counter.to_le_bytes());
        let digest = finish64(state);
        let bytes = digest.as_bytes();
        let word = u64::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]]);
        let leaf = (word % leaf_count as u64) as u32;
        if !picked.contains(&leaf) {
            picked.push(leaf);
        }
        counter += 1;
    }
    picked.sort_unstable();
    picked
}

/// The V2 challenge's seed: the class, the bond and the span, under the V2 domain (so a V1 seed is
/// never a V2 seed, whatever a relayer replays).
pub fn palw_readiness_v2_challenge_seed_v1(class_id: &Hash64, bond: &[u8], span: u64) -> Hash64 {
    let mut state = keyed64(PALW_SEAT_READINESS_V2_CHALLENGE_DOMAIN);
    state.update(class_id.as_byte_slice());
    state.update(&(bond.len() as u64).to_le_bytes());
    state.update(bond);
    state.update(&span.to_le_bytes());
    finish64(state)
}

/// **What a V2 proof's signature covers**: the network, the bond, the class, the span, the
/// inventory's size and **the bytes it opened** — the last of which V1's message omitted, so one
/// seat's published opening could be re-signed by another bond that never held the data.
pub fn palw_seat_readiness_message_v2(
    network_domain: Hash64,
    bond: &[u8],
    class_id: &Hash64,
    span: u64,
    proof: &crate::palw_artifact::PalwArtifactMultiproofV1,
) -> Hash64 {
    let mut state = keyed64(PALW_SEAT_READINESS_V2_DOMAIN);
    state.update(network_domain.as_byte_slice());
    state.update(&(bond.len() as u64).to_le_bytes());
    state.update(bond);
    state.update(class_id.as_byte_slice());
    state.update(&span.to_le_bytes());
    state.update(&proof.leaf_count.to_le_bytes());
    state.update(&(proof.opened.len() as u64).to_le_bytes());
    for (index, operand) in &proof.opened {
        state.update(&index.to_le_bytes());
        state.update(crate::palw_artifact::artifact_leaf_v1(operand).as_byte_slice());
    }
    finish64(state)
}

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

/// ADR-0135 manifest V2: the domain of the registrant's byte-count commitment.
pub const PALW_CLASS_MANIFEST_V2_DOMAIN: &[u8] = b"misaka-palw/class-manifest-v2/message/v1";
/// ADR-0135 manifest V2: the ML-DSA-87 context the registrant signs the commitment under.
pub const PALW_CLASS_MANIFEST_V2_MLDSA87_CONTEXT: &[u8] = b"misaka-palw/class-manifest-v2/mldsa87/v1";

/// **ADR-0135 manifest V2: what the registrant signs when it commits the artifact's byte count.**
/// The network domain, the registrant bond, the class and the count — nothing else, so a
/// signature is good for exactly one (class, count) on one chain, and a re-measured file is a new
/// signature rather than a replay.
pub fn palw_class_manifest_message_v2(network_domain: Hash64, bond: &[u8], class_id: &Hash64, artifact_bytes: u64) -> Hash64 {
    let mut state = keyed64(PALW_CLASS_MANIFEST_V2_DOMAIN);
    state.update(network_domain.as_byte_slice());
    state.update(&(bond.len() as u64).to_le_bytes());
    state.update(bond);
    state.update(class_id.as_byte_slice());
    state.update(&artifact_bytes.to_le_bytes());
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
    /// ADR-0137 (shadow): the work target and the panel's replay budget, where the shadow folds.
    pub work_target: Option<PalwWorkTargetReadV1>,
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
        PalwModelLifecycleV1::Candidate => {
            "candidate: registered, and not admitted — no seat outside the registrant's own identity has proved it is ready to \
             verify this class"
                .to_string()
        }
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
                // **The number the DECISION used, said to be that** (audit 2026-09-19). The row's
                // `ready_seats` is what the rule read when it held the class; the readout beside
                // this line reports `readySeatsNow`, recomputed. Both are honest and they disagree
                // the moment seats come back, so an operator reading "ready 0" next to "8" cannot
                // tell whether the class is stuck or recovering. Naming the reading is the whole
                // fix; this repository already has the rule that a status must carry its provenance.
                format!(
                    "held: {} seats were ready when it was held, {seats} are needed for a panel (recovers through probation once \
                     {} are ready — compare readySeatsNow for what is ready right now)",
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
    /// ADR-0137 (shadow): the class's economic compute a claim (its CCU), `CCU / W` in permille,
    /// the expected forwards a win (Q32), the ticket the work target would set beside the class
    /// target the shipped rule sets, the panel room in this class's claims, and the reader's share
    /// of finalized work over ten and a hundred epochs.
    pub economic_ccu_per_claim: u128,
    pub work_ratio_permille: u32,
    pub expected_forwards_q32: u128,
    pub work_ticket_target: u128,
    pub class_target: u128,
    pub panel_room: u64,
    pub final_work_share_10_permille: u16,
    pub final_work_share_100_permille: u16,
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
/// ADR-0137 (shadow): what op 186 prints of the work target — the state's shadow row, the rate it
/// was floored with, the panel's in-flight replay and the budget's horizon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwWorkTargetReadV1 {
    pub target: crate::palw_work_target_v1::PalwWorkTargetV2,
    pub rate_sompi_per_giga: u64,
    pub panel_inflight_replay: u128,
    pub panel_horizon_spans: u64,
    pub final_work_epochs: u64,
}

pub fn palw_model_registry_read_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    params: &crate::palw_state_v2::PalwStateParamsV2,
    tip_daa: u64,
    fence_daa: Option<u64>,
    fold: Option<&PalwModelRegistryFoldV1>,
    work: Option<&crate::palw_work_target_v1::PalwWorkTargetFoldV1>,
) -> PalwModelRegistryReadV1 {
    use crate::palw_work_target_v1 as wt;
    let base = params.base_class_id();
    // ADR-0137 (shadow): every class's CCU (the fold's works, else its row's), the network-wide
    // in-flight replay, the budget's common horizon (the shortest admitted window) and the
    // reader's shares — computed once, read per class below.
    let shadow = state.work_target_shadow().copied();
    let g = fold.map(|f| f.globals).unwrap_or(PALW_REGISTRY_GLOBALS_V1);
    let ccu_of = |class_id: &Hash64| -> u128 {
        work.and_then(|w| w.works.get(class_id))
            .map(|w| w.economic_ccu_per_claim)
            .or_else(|| state.model_lifecycle(class_id).map(|row| row.work.economic_ccu_per_claim))
            .unwrap_or(0)
    };
    let seat_count = g.seat_count as u128;
    let panel_inflight_replay: u128 = state
        .classes_iter()
        .filter(|(id, _)| **id != base)
        .map(|(id, _)| (palw_model_registry_inflight_v1(state, id) as u128).saturating_mul(ccu_of(id)).saturating_mul(seat_count))
        .fold(0u128, u128::saturating_add);
    let panel_horizon_spans = state
        .model_lifecycles_iter()
        .filter(|(id, row)| **id != base && row.state.admission_permille() > 0)
        .map(|(_, row)| row.profile.verification_window_spans as u64)
        .min()
        .unwrap_or(1)
        .max(1);
    let shares_10 = state.final_work_shares_v1(10);
    let shares_100 = state.final_work_shares_v1(100);
    let share_of =
        |shares: &[(Hash64, u16)], class_id: &Hash64| shares.iter().find(|(id, _)| id == class_id).map(|(_, s)| *s).unwrap_or(0);
    let classes = state
        .classes_iter()
        .map(|(class_id, record)| PalwModelRegistryClassReadV1 {
            economic_ccu_per_claim: ccu_of(class_id),
            work_ratio_permille: shadow.map(|t| wt::palw_work_ratio_permille_v1(ccu_of(class_id), t.work)).unwrap_or(0),
            expected_forwards_q32: shadow.map(|t| wt::palw_expected_forwards_q32_v1(ccu_of(class_id), t.work)).unwrap_or(0),
            work_ticket_target: shadow.map(|t| wt::palw_work_ticket_target_v1(ccu_of(class_id), t.work)).unwrap_or(0),
            class_target: state.class_target(class_id).map(|t| t.target).unwrap_or(0),
            panel_room: if *class_id == base {
                0
            } else {
                let ready = fold.map(|f| palw_model_registry_ready_seats_v1(state, params, class_id, tip_daa, f)).unwrap_or(0) as u128;
                let per_span =
                    ready.saturating_mul(g.reference_work_per_span).saturating_mul(g.utilization_permille.min(1_000) as u128) / 1_000;
                wt::palw_panel_room_v1(
                    per_span,
                    panel_horizon_spans,
                    panel_inflight_replay,
                    ccu_of(class_id).saturating_mul(seat_count),
                )
            },
            final_work_share_10_permille: share_of(&shares_10, class_id),
            final_work_share_100_permille: share_of(&shares_100, class_id),
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
            // **A `Candidate` is counted where an operator already looks for "on chain and
            // admitting nothing".** A seventh slot would widen `counts` and with it the RPC row
            // (`classes_registered` and its five siblings are named fields), which is a wire break
            // for a state that cannot exist below a dormant fence. The distinction an operator
            // needs is in the class's own reason line, which names independence by name.
            PalwModelLifecycleV1::Candidate => 4,
        };
        counts[slot] += 1;
    }
    PalwModelRegistryReadV1 {
        work_target: shadow.map(|target| PalwWorkTargetReadV1 {
            target,
            rate_sompi_per_giga: work.map(|w| w.rate_sompi_per_giga).unwrap_or(0),
            panel_inflight_replay,
            panel_horizon_spans,
            final_work_epochs: state.final_work_iter().count() as u64,
        }),
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
/// **A class registered before the registry fence never gets a lifecycle row** (ADR-0135 with the
/// 2026-09-18 audit's C-1: the fold's works come from the chain and the build, never from a node's
/// own carriage store), so a node that would register a class on a chain whose registry fence is
/// scheduled but not yet in force must WAIT for the fence — the object it would send is a class
/// with a share and no row, which the work target then refuses for ever ("no row"). `true` while
/// the registration must wait.
pub fn palw_registration_waits_for_registry_v1(registry_fence: Option<crate::config::params::ForkActivation>, daa_score: u64) -> bool {
    registry_fence.is_some_and(|fence| fence != crate::config::params::ForkActivation::never() && !fence.is_active(daa_score))
}

/// **How many DAA a registration's carrier may take to land** — the window before the admission-
/// independence fence in which a node does not build one (see
/// [`palw_registration_waits_for_fences_v2`]). A carrier is accepted by the next chain block that
/// merges it; ten DAA is several blocks on every preset, and a carrier that lands later still is
/// caught by the panel's own retry, which rebuilds it on the terms then in force.
pub const PALW_REGISTRATION_LANDING_MARGIN_DAA_V1: u64 = 10;

/// [`palw_registration_waits_for_registry_v1`], and **the admission-independence fence too**
/// (ADR-0147 §2.4 at the entrance; the 2026-09-20 Studio economy drill).
///
/// Below `palw_admission_independence` a post-genesis registration must take the minimum grantable
/// share, past it exactly 0‰ — registration buys existence, cadence is earned — and the gate reads
/// the fence at the block that ACCEPTS the carrier. A registration built in the last blocks before
/// the fence therefore carries a share the fence refuses when the carrier lands past it. The drill
/// that armed the registry and the bundle at one height found exactly that: the node registered the
/// moment the registry opened, the object was dropped "registers at 1‰", and the panel's retry only
/// came 200 DAA later. So the node also waits while the independence fence is scheduled and the
/// chain is within [`PALW_REGISTRATION_LANDING_MARGIN_DAA_V1`] below it, and registers once the
/// fence is in force — when the terms say 0‰ and the gate agrees.
pub fn palw_registration_waits_for_fences_v2(
    registry_fence: Option<crate::config::params::ForkActivation>,
    independence_fence: Option<crate::config::params::ForkActivation>,
    daa_score: u64,
) -> bool {
    if palw_registration_waits_for_registry_v1(registry_fence, daa_score) {
        return true;
    }
    independence_fence.is_some_and(|fence| {
        fence != crate::config::params::ForkActivation::never()
            && !fence.is_active(daa_score)
            && daa_score.saturating_add(PALW_REGISTRATION_LANDING_MARGIN_DAA_V1) >= fence.daa_score()
    })
}

pub fn palw_readiness_duty_due_v1(
    row: Option<&PalwSeatReadinessRowV1>,
    now_daa: u64,
    span_now: u64,
    last_submitted_span: Option<u64>,
    span_daa: u64,
    g: &PalwRegistryGlobalsV1,
) -> bool {
    palw_readiness_duty_due_v2(row, now_daa, span_now, last_submitted_span, span_daa, g, false)
}

/// **ADR-0133 §11.2: the same question past readiness V2.** The chain stops counting a V1 row the
/// moment the fence bites and gives a V2 row eight spans instead of thirty — so a seat that asked
/// the old question would sit on a fresh-looking V1 row while the registry counted it out, lose its
/// seat, and take the class's panel with it. Past the fence a V1 row is always due, and a V2 row is
/// due at half the V2 window.
pub fn palw_readiness_duty_due_v2(
    row: Option<&PalwSeatReadinessRowV1>,
    now_daa: u64,
    span_now: u64,
    last_submitted_span: Option<u64>,
    span_daa: u64,
    g: &PalwRegistryGlobalsV1,
    readiness_v2: bool,
) -> bool {
    if last_submitted_span == Some(span_now) {
        return false;
    }
    let spans = if readiness_v2 { PALW_READINESS_V2_MAX_AGE_SPANS_V1 } else { g.readiness_probe_max_age_spans };
    let age_daa = (spans as u64).saturating_mul(span_daa.max(1));
    match row {
        None => true,
        Some(row) if readiness_v2 && row.proof_version < 2 => true, // a one-leaf row counts for nothing now
        Some(row) => now_daa.saturating_sub(row.proved_daa) > age_daa / 2,
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_registration_waits_for_a_scheduled_registry_fence_and_never_for_an_absent_one() {
        use crate::config::params::ForkActivation;
        assert!(!palw_registration_waits_for_registry_v1(None, 5), "no fence: nothing to wait for");
        assert!(!palw_registration_waits_for_registry_v1(Some(ForkActivation::never()), 5), "never: nothing to wait for");
        assert!(palw_registration_waits_for_registry_v1(Some(ForkActivation::new(20)), 19), "scheduled, not yet: wait");
        assert!(!palw_registration_waits_for_registry_v1(Some(ForkActivation::new(20)), 20), "in force: go");
        assert!(!palw_registration_waits_for_registry_v1(Some(ForkActivation::always()), 0), "always: go");
    }

    /// The Studio economy drill's finding: with the registry and the bundle at one height, the node
    /// registered the moment the registry opened, on the pre-independence share, and the carrier
    /// landed past the independence fence, which refused it. A registration is not built within a
    /// landing margin below the independence fence, and is built once the fence is in force.
    #[test]
    fn a_registration_is_not_built_where_its_carrier_could_land_on_the_other_side_of_independence() {
        use crate::config::params::ForkActivation;
        let registry = Some(ForkActivation::new(30));
        let independence = Some(ForkActivation::new(30));
        let margin = PALW_REGISTRATION_LANDING_MARGIN_DAA_V1;
        assert!(palw_registration_waits_for_fences_v2(registry, independence, 29), "the registry is not open yet");
        assert!(!palw_registration_waits_for_fences_v2(registry, independence, 30), "both in force: build on the 0 permille terms");
        // The registry open well before the bundle: register freely, until the margin before it.
        let registry = Some(ForkActivation::new(20));
        let independence = Some(ForkActivation::new(100));
        assert!(!palw_registration_waits_for_fences_v2(registry, independence, 25), "far from the fence: the carrier lands before it");
        assert!(!palw_registration_waits_for_fences_v2(registry, independence, 100 - margin - 1));
        assert!(palw_registration_waits_for_fences_v2(registry, independence, 100 - margin), "inside the landing margin: wait");
        assert!(palw_registration_waits_for_fences_v2(registry, independence, 99), "the last block before the fence: wait");
        assert!(!palw_registration_waits_for_fences_v2(registry, independence, 100), "the fence in force: build");
        // No independence fence scheduled: the registry is the only thing waited for.
        assert!(!palw_registration_waits_for_fences_v2(registry, None, 99));
        assert!(!palw_registration_waits_for_fences_v2(registry, Some(ForkActivation::never()), 99));
    }
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
    fn adr0133_a_class_whose_replay_does_not_fit_the_receipt_deadline_is_held_never_active() {
        // **§11.3, fail-closed.** The observation says the derived window is wider than the
        // network's receipt deadline: from every admitting state the class goes to Held, and Held
        // stays Held however ready its seats are, until the window fits again.
        use PalwModelLifecycleV1::*;
        let work = PalwModelWorkV1 {
            verification_ccu: 2_000_000_000_000,
            economic_ccu_per_claim: 1_000_000_000_000,
            ops_supported: true,
            ..Default::default()
        };
        let k = palw_lifecycle_profile_v1(&work, 1 << 32, &G);
        let obs = |fits: bool, state_ok: bool| PalwLifecycleObservationV1 {
            manifest: PalwManifestVerdictV1Flag::Valid,
            ready_seats: 7,
            probes_passed_this_span: 1,
            probes_failed_this_span: 0,
            utilization_permille: 300,
            collateral_ok: state_ok,
            cap_ok: true,
            window_fits_receipt: fits,
            span_stable: true,
            // ADR-0145 §7: the `Candidate` arm's only input, and this fixture starts past it.
            admission_jury_seated: false,
        };
        for from in [Probation { probes_passed: 9 }, ActiveLimited { stable_epochs: 9 }, Active] {
            assert_eq!(palw_lifecycle_step_v1(from, &obs(false, true), &k, &G), Held, "{from:?}: does not fit → Held");
        }
        assert_eq!(palw_lifecycle_step_v1(Held, &obs(false, true), &k, &G), Held, "Held stays Held while it does not fit");
        assert_eq!(
            palw_lifecycle_step_v1(Held, &obs(true, true), &k, &G),
            Probation { probes_passed: 0 },
            "…and re-enters Probation once it fits"
        );
        assert_eq!(palw_lifecycle_step_v1(Active, &obs(true, true), &k, &G), Active, "a fitting window changes nothing");
    }

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
            window_fits_receipt: true,
            span_stable: true,
            // Every fixture below this line walks a class that is already past `Candidate`, and
            // `admission_jury_seated` is read by that one arm: `false` here says so, and says that
            // nothing else in the lifecycle learned to read it.
            admission_jury_seated: false,
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
        let fresh = PalwSeatReadinessRowV1 { proved_daa: 990, proved_span: 198, leaf_index: 3, proof_version: 1, chunks: 1 };
        assert!(!palw_readiness_duty_due_v1(Some(&fresh), 1_000, 200, None, span, &g), "a fresh proof is not repeated");
        let half = PalwSeatReadinessRowV1 {
            proved_daa: 1_000 - (g.readiness_probe_max_age_spans as u64 * span) / 2 - 1,
            proved_span: 0,
            leaf_index: 3,
            proof_version: 1,
            chunks: 1,
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
            window_fits_receipt: true,
            span_stable: true,
            // Every fixture below this line walks a class that is already past `Candidate`, and
            // `admission_jury_seated` is read by that one arm: `false` here says so, and says that
            // nothing else in the lifecycle learned to read it.
            admission_jury_seated: false,
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

    /// **The typed catalog describes the four shipped rows to the registry**, each with a non-zero
    /// draw and verification compute — the works testnet-11's genesis classes get, since no bundle
    /// registration carries a carriage for them.
    #[test]
    fn adr0135_the_typed_catalog_describes_the_shipped_rows() {
        let works = palw_rc_typed_class_works_v1();
        let ids: Vec<String> = works.keys().map(|id| id.to_string()[..16].to_string()).collect();
        for expected in ["f1c5635c6e47e96e", "5bd9ae3d91df8065", "4277d84f7d91528c", "2705b8f65f7ba54a"] {
            assert!(ids.contains(&expected.to_string()), "{expected} is described; have {ids:?}");
        }
        for (id, work) in &works {
            assert!(work.ops_supported && work.economic_ccu_per_claim > 0 && work.verification_ccu > 0, "{id}: {work:?}");
        }
        assert_eq!(works, palw_rc_typed_class_works_v1(), "a function of the binary alone");
    }

    /// The landing allowance is eight spans or forty DAA, whichever is more: testnet-11's five-DAA
    /// spans keep the eight, a two-DAA devnet gets twenty, a one-DAA fixture forty.
    #[test]
    fn adr0135_the_landing_allowance_is_the_larger_of_eight_spans_and_forty_daa() {
        assert_eq!(palw_readiness_landing_spans_v1(5), 8);
        assert_eq!(palw_readiness_landing_spans_v1(10), 8);
        assert_eq!(palw_readiness_landing_spans_v1(2), 20);
        assert_eq!(palw_readiness_landing_spans_v1(1), 40);
        assert_eq!(palw_readiness_landing_spans_v1(0), 40, "a zero span is read as one");
    }
}
