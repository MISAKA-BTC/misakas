//! **ADR-0133 — verification is its own time axis: a class's verification window, its panel's
//! capacity, and the simulation that sizes them.** Shadow: nothing consensus reads.
//!
//! Three clocks that the design keeps apart (ADR-0133 §3): the execution lane's round (one
//! second), the PALW anchor cadence (a chain block, 120 s on testnet-11), and a class's
//! verification window (whole execution spans). The receipt window a panel already has is 600 DAA
//! — three hundred anchors — so no live class is bounded by one anchor today; what bounds a large
//! model is the panel's capacity (Little's law over the window), the artifact's residency, and the
//! collateral the inflight duties hold. This module derives a class's
//! [`PalwVerificationProfileV1`] from what its registration states and what calibration measured
//! (never from what its producer would like), sizes a panel by the profile, and runs the
//! profile × artifact grid the operator asked for.
//!
//! Every quantity is an integer: milliseconds, bytes, MAC-equivalents, permille, and claim counts
//! in thousandths (`_milli`) where a rate is fractional.

use crate::palw_execution_lane_v1::{PalwExecFinalV1, PalwExecSnapshotV1, palw_execution_schedule_snapshot_v1};

pub const PALW_VERIFICATION_PROFILE_VERSION_V1: u16 = 1;

/// One PALW anchor on testnet-11: the chain's 120-second cadence.
pub const PALW_ANCHOR_PERIOD_MS_V1: u64 = 120_000;
/// One execution span on testnet-11 (ADR-0130 Decision 5): five anchors.
pub const PALW_SPAN_ANCHORS_V1: u64 = 5;
pub const PALW_SPAN_MS_V1: u64 = PALW_ANCHOR_PERIOD_MS_V1 * PALW_SPAN_ANCHORS_V1;
/// ADR-0124's panel: five seats, three to license.
pub const PALW_SEAT_COUNT_V1: u16 = 5;
pub const PALW_QUORUM_V1: u16 = 3;
/// The receipt window a panel holds today (`PALW_RC_WINDOWS_V1.window_receipt`), in anchors.
pub const PALW_RECEIPT_WINDOW_ANCHORS_V1: u64 = 600;
/// The challenge window a licence waits before `Final` (`window_challenge`), in anchors.
pub const PALW_CHALLENGE_WINDOW_ANCHORS_V1: u64 = 1_200;

/// **What a class states at registration and what its calibration measured.** The registrant
/// declares the artifact's size; the compute is the graph's (ADR-0131); the two p99s come from the
/// shadow period (ADR-0131 Decision 6) — a warm one (the artifact resident, the working set in
/// memory) and a cold one (the artifact read from storage during the replay). Zero for a p99 means
/// "not measured": the reference rates estimate it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwClassTimingFactsV1 {
    pub draw_compute: u128,
    pub artifact_bytes: u64,
    pub warm_p99_ms: u64,
    pub cold_p99_ms: u64,
}

/// **The reference a network states** — consensus constants once a profile fence is armed, shadow
/// constants until then: a reference verifier's MAC-equivalents per millisecond and bytes read per
/// millisecond, the allowance for a receipt to propagate, be carried into a block and count toward a
/// quorum, the safety factor, the utilization a panel is sized to, and the fewest eligible seats a
/// class needs before it is live at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwVerificationReferenceV1 {
    pub mac_eq_per_ms: u64,
    pub artifact_bytes_per_ms: u64,
    pub receipt_allowance_ms: u64,
    pub safety_permille: u32,
    pub utilization_permille: u32,
    pub min_eligible_seats: u16,
}

/// The reference measured on the fleet, 2026-09-17: the ibm seat replays the dense tier's 83.1 G
/// MAC-eq job in 20 s (4 G MAC-eq/s); a resident NVMe reads a gigabyte a second; a receipt is
/// carried within two anchors; twice the p99; seventy percent utilization; seven eligible seats
/// (a panel plus two, so one operator down still draws a full panel).
pub const PALW_VERIFICATION_REFERENCE_V1: PalwVerificationReferenceV1 = PalwVerificationReferenceV1 {
    mac_eq_per_ms: 4_000_000,
    artifact_bytes_per_ms: 1_000_000,
    receipt_allowance_ms: 2 * PALW_ANCHOR_PERIOD_MS_V1,
    safety_permille: 2_000,
    utilization_permille: 700,
    min_eligible_seats: PALW_SEAT_COUNT_V1 + 2,
};

/// **A class's verification profile** — fixed at activation, never chosen by a producer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwVerificationProfileV1 {
    pub version: u16,
    /// Whole execution spans a panel has to license before the class's claim is redrawn.
    pub verification_window_spans: u32,
    /// Spans before a duty a seat must have the artifact resident to verify warm.
    pub artifact_prefetch_spans: u32,
    /// The most claims of the class that may be bound at once (ADR-0133 Decision 3: a class that
    /// outruns its panel holds its own new claims, and only its own).
    pub max_inflight_claims: u32,
    pub warm_p99_ms: u64,
    pub cold_p99_ms: u64,
    pub seat_count: u16,
}

fn div_ceil(a: u64, b: u64) -> u64 {
    if b == 0 { 0 } else { a.div_ceil(b) }
}

/// The reference's estimate of a warm replay: the job's compute over the verifier's rate.
pub fn palw_warm_estimate_ms_v1(facts: &PalwClassTimingFactsV1, reference: &PalwVerificationReferenceV1) -> u64 {
    (facts.draw_compute / reference.mac_eq_per_ms.max(1) as u128).min(u64::MAX as u128) as u64
}

/// The reference's estimate of a cold replay: warm plus the artifact read once from storage.
pub fn palw_cold_estimate_ms_v1(facts: &PalwClassTimingFactsV1, reference: &PalwVerificationReferenceV1) -> u64 {
    palw_warm_estimate_ms_v1(facts, reference).saturating_add(facts.artifact_bytes / reference.artifact_bytes_per_ms.max(1))
}

/// A registrant cannot shrink its window below the reference's estimates (`palw_verification_profile_v1`).
pub fn palw_profile_p99s_v1(facts: &PalwClassTimingFactsV1, reference: &PalwVerificationReferenceV1) -> (u64, u64) {
    let warm = facts.warm_p99_ms.max(palw_warm_estimate_ms_v1(facts, reference)).max(1);
    let cold = facts.cold_p99_ms.max(warm.saturating_add(facts.artifact_bytes / reference.artifact_bytes_per_ms.max(1)));
    (warm, cold)
}

/// **Derive the profile** (ADR-0133 Decision 2). The p99 used is the larger of the measured one
/// and the reference's estimate, so a registrant cannot shrink its window by under-reporting;
/// `window = ⌈safety × (cold_p99 + receipt allowance) / span⌉` spans (one at least), the prefetch
/// is the artifact's cold read at the same safety, and the inflight cap is Little's law at the
/// target utilization over the fewest eligible seats: `⌊utilization × seats_min × window /
/// (seat_count × warm_p99)⌋` claims, one at least.
pub fn palw_verification_profile_v1(
    facts: &PalwClassTimingFactsV1,
    reference: &PalwVerificationReferenceV1,
    seat_count: u16,
) -> PalwVerificationProfileV1 {
    let warm_p99_ms = facts.warm_p99_ms.max(palw_warm_estimate_ms_v1(facts, reference)).max(1);
    // A cold replay is at least the warm one plus the artifact read once from storage, whatever
    // was measured: the read is the reference's floor on the cache miss.
    let artifact_read_ms = facts.artifact_bytes / reference.artifact_bytes_per_ms.max(1);
    let cold_p99_ms = facts.cold_p99_ms.max(warm_p99_ms.saturating_add(artifact_read_ms));
    let safety = reference.safety_permille.max(1_000) as u64;
    let window_ms = cold_p99_ms.saturating_add(reference.receipt_allowance_ms).saturating_mul(safety) / 1_000;
    let verification_window_spans = div_ceil(window_ms, PALW_SPAN_MS_V1).max(1);
    let prefetch_ms = (facts.artifact_bytes / reference.artifact_bytes_per_ms.max(1)).saturating_mul(safety) / 1_000;
    let artifact_prefetch_spans = if facts.artifact_bytes == 0 { 0 } else { div_ceil(prefetch_ms, PALW_SPAN_MS_V1).max(1) };
    let window_ms_whole = verification_window_spans * PALW_SPAN_MS_V1;
    let seats_min = reference.min_eligible_seats.max(seat_count) as u64;
    let cap = (reference.utilization_permille.min(1_000) as u64).saturating_mul(seats_min).saturating_mul(window_ms_whole)
        / 1_000
        / (seat_count.max(1) as u64 * warm_p99_ms);
    PalwVerificationProfileV1 {
        version: PALW_VERIFICATION_PROFILE_VERSION_V1,
        verification_window_spans: verification_window_spans.min(u32::MAX as u64) as u32,
        artifact_prefetch_spans: artifact_prefetch_spans.min(u32::MAX as u64) as u32,
        max_inflight_claims: cap.clamp(1, u32::MAX as u64) as u32,
        warm_p99_ms,
        cold_p99_ms,
        seat_count,
    }
}

/// **A class's panel over one span** (ADR-0133 §5): its accepted-claim rate, its profile, how many
/// seats can judge it, how often a replay is cold, and what one seat reserves per duty.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwPanelCapacityInputV1 {
    /// Claims accepted a span, in thousandths.
    pub accepted_per_span_milli: u64,
    pub profile: PalwVerificationProfileV1,
    pub eligible_seats: u32,
    /// How many of a class's replays run cold, in permille (a prefetching fleet: ~0).
    pub cold_fraction_permille: u32,
    pub seat_exposure_sompi: u128,
}

/// What the panel's arithmetic says.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwPanelCapacityV1 {
    /// One replay's mean service time at the cold fraction.
    pub service_ms: u64,
    /// Claims in flight over the window (Little), in thousandths.
    pub inflight_claims_milli: u64,
    /// Seat-milliseconds the class demands a span: rate × seats × service.
    pub load_ms_per_span: u64,
    /// Seat-milliseconds the eligible seats offer a span.
    pub capacity_ms_per_span: u64,
    pub utilization_permille: u32,
    /// The accepted rate the panel could carry at the target utilization, in thousandths.
    pub max_accepted_per_span_milli: u64,
    /// Σ seat exposure the inflight duties hold.
    pub reserved_exposure_sompi: u128,
    /// The class's inflight cap against its inflight: the claims the gate would hold, in thousandths.
    pub held_by_cap_milli: u64,
    /// `Final` latency in spans: the window (a licence at its end, worst case) plus the challenge
    /// window — which is the term that dominates, and is the same for every class.
    pub final_latency_spans: u64,
    /// Seats needed at the target utilization, and with one and two operators down.
    pub seats_required: u32,
    pub seats_required_n1: u32,
    pub seats_required_n2: u32,
    /// Whether the class is live: a panel can be drawn (eligible seats ≥ the panel) and the
    /// utilization is under one.
    pub live: bool,
    /// Eligible seats beyond the panel: how many operators may go down before the class is dead.
    pub outage_margin_seats: u32,
}

impl PalwPanelCapacityV1 {
    pub fn within_target(&self, reference: &PalwVerificationReferenceV1) -> bool {
        self.utilization_permille <= reference.utilization_permille
    }
}

/// [`PalwPanelCapacityV1`] of a class.
pub fn palw_panel_capacity_v1(input: &PalwPanelCapacityInputV1, reference: &PalwVerificationReferenceV1) -> PalwPanelCapacityV1 {
    let p = &input.profile;
    let cold_extra = p.cold_p99_ms.saturating_sub(p.warm_p99_ms);
    let service_ms = p.warm_p99_ms.saturating_add(cold_extra.saturating_mul(input.cold_fraction_permille.min(1_000) as u64) / 1_000);
    let window_spans = p.verification_window_spans.max(1) as u64;
    let inflight_claims_milli = input.accepted_per_span_milli.saturating_mul(window_spans);
    let load_ms_per_span = input.accepted_per_span_milli.saturating_mul(p.seat_count.max(1) as u64).saturating_mul(service_ms) / 1_000;
    let capacity_ms_per_span = (input.eligible_seats as u64).saturating_mul(PALW_SPAN_MS_V1);
    let utilization_permille = if capacity_ms_per_span == 0 {
        u32::MAX
    } else {
        (load_ms_per_span.saturating_mul(1_000) / capacity_ms_per_span).min(u32::MAX as u64) as u32
    };
    let per_claim_ms = (p.seat_count.max(1) as u64).saturating_mul(service_ms).max(1);
    let max_accepted_per_span_milli =
        capacity_ms_per_span.saturating_mul(reference.utilization_permille.min(1_000) as u64) / per_claim_ms;
    let reserved_exposure_sompi =
        (inflight_claims_milli as u128).saturating_mul(p.seat_count as u128).saturating_mul(input.seat_exposure_sompi) / 1_000;
    let cap_milli = (p.max_inflight_claims as u64).saturating_mul(1_000);
    let held_by_cap_milli = inflight_claims_milli.saturating_sub(cap_milli);
    let final_latency_spans = window_spans + div_ceil(PALW_CHALLENGE_WINDOW_ANCHORS_V1, PALW_SPAN_ANCHORS_V1);
    let seats_required =
        div_ceil(load_ms_per_span.saturating_mul(1_000), PALW_SPAN_MS_V1.saturating_mul(reference.utilization_permille.max(1) as u64))
            .max(p.seat_count as u64)
            .min(u32::MAX as u64) as u32;
    let live = input.eligible_seats >= p.seat_count as u32 && utilization_permille < 1_000;
    let outage_margin_seats = input.eligible_seats.saturating_sub(p.seat_count as u32);
    PalwPanelCapacityV1 {
        service_ms,
        inflight_claims_milli,
        load_ms_per_span,
        capacity_ms_per_span,
        utilization_permille,
        max_accepted_per_span_milli,
        reserved_exposure_sompi,
        held_by_cap_milli,
        final_latency_spans,
        seats_required,
        seats_required_n1: seats_required + 1,
        seats_required_n2: seats_required + 2,
        live,
        outage_margin_seats,
    }
}

/// **The class-local gate** (ADR-0133 Decision 3): a claim of a class whose bound claims already
/// number its cap is held — not bound, not refused, not voided until its own bind window runs out —
/// and no other class's claim is touched. Pure; the fold applies it at the anchor slot when the
/// profile fence is armed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwClassGateV1 {
    Bind,
    Hold,
}

pub fn palw_class_gate_v1(inflight_bound: u32, max_inflight_claims: u32) -> PalwClassGateV1 {
    if max_inflight_claims == 0 || inflight_bound < max_inflight_claims { PalwClassGateV1::Bind } else { PalwClassGateV1::Hold }
}

/// **A network of classes over spans**, for the property the operator named: one class's
/// starvation moves nothing outside that class. Each class is simulated on its own queue; the
/// execution lane's schedule is the real one (`palw_execution_schedule_snapshot_v1`) over the
/// `Final`s the classes produce; the anchor cadence is a constant no class state enters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwClassSimV1 {
    pub domain: crate::Hash64,
    pub profile: PalwVerificationProfileV1,
    /// Claims accepted a span, in thousandths.
    pub accepted_per_span_milli: u64,
    pub eligible_seats: u32,
    pub cold_fraction_permille: u32,
    pub seat_exposure_sompi: u128,
    /// The bond and operator the class's finals are credited to in the lane (one producer).
    pub bond: crate::palw_state_v2::PalwBondKeyV2,
    pub operator_id: crate::Hash64,
}

/// One class's state after `spans` spans.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwClassSimStateV1 {
    pub accepted_milli: u64,
    pub bound_milli: u64,
    pub held_milli: u64,
    pub licensed_milli: u64,
    pub finals_milli: u64,
    pub voided_milli: u64,
    pub inflight_milli: u64,
    pub utilization_permille: u32,
    pub reserved_exposure_sompi: u128,
    pub live: bool,
}

/// The network after `spans` spans, with the lane's schedule for the span after.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwNetworkSimV1 {
    pub classes: Vec<PalwClassSimStateV1>,
    pub schedule: PalwExecSnapshotV1,
    /// The anchor cadence, which no class touches.
    pub anchor_period_ms: u64,
    pub spans: u64,
}

impl PalwNetworkSimV1 {
    /// The lane's domain for `domain`, if any class credited it a `Final`.
    pub fn domain(&self, domain: &crate::Hash64) -> Option<&crate::palw_execution_lane_v1::PalwExecDomainV1> {
        self.schedule.domains.iter().find(|d| d.domain == *domain)
    }
}

/// Run `spans` spans. Per span and class: the class accepts its rate; the gate binds up to its cap
/// (the rest is held); a bound claim is licensed at the end of its window if its panel's utilization
/// is under one and the seats suffice, else it voids at the window's end; a licence becomes a
/// `Final` after the challenge window; every `Final` is a lane credit. Fluid (thousandths), so a
/// rate below one claim a span still flows.
pub fn palw_network_sim_v1(classes: &[PalwClassSimV1], spans: u64, reference: &PalwVerificationReferenceV1) -> PalwNetworkSimV1 {
    let challenge_spans = div_ceil(PALW_CHALLENGE_WINDOW_ANCHORS_V1, PALW_SPAN_ANCHORS_V1);
    let mut states = vec![PalwClassSimStateV1::default(); classes.len()];
    // Per class: queues of (span due, amount) for bound claims and licences.
    let mut bound_queues: Vec<Vec<(u64, u64)>> = vec![Vec::new(); classes.len()];
    let mut licence_queues: Vec<Vec<(u64, u64)>> = vec![Vec::new(); classes.len()];
    let mut finals: Vec<PalwExecFinalV1> = Vec::new();
    for span in 0..spans {
        for (i, class) in classes.iter().enumerate() {
            let state = &mut states[i];
            let capacity = palw_panel_capacity_v1(
                &PalwPanelCapacityInputV1 {
                    accepted_per_span_milli: class.accepted_per_span_milli,
                    profile: class.profile,
                    eligible_seats: class.eligible_seats,
                    cold_fraction_permille: class.cold_fraction_permille,
                    seat_exposure_sompi: class.seat_exposure_sompi,
                },
                reference,
            );
            state.utilization_permille = capacity.utilization_permille;
            state.live = capacity.live;
            // Bound claims due this span license or void.
            let mut due = 0u64;
            bound_queues[i].retain(|(at, amount)| {
                if *at <= span {
                    due += amount;
                    false
                } else {
                    true
                }
            });
            if due > 0 {
                if capacity.live {
                    state.licensed_milli += due;
                    licence_queues[i].push((span + challenge_spans, due));
                } else {
                    state.voided_milli += due;
                }
            }
            let mut finalized = 0u64;
            licence_queues[i].retain(|(at, amount)| {
                if *at <= span {
                    finalized += amount;
                    false
                } else {
                    true
                }
            });
            if finalized > 0 {
                state.finals_milli += finalized;
                // Whole finals credit the lane; the fraction carries over in the fluid count.
                let whole = state.finals_milli / 1_000 - (state.finals_milli - finalized) / 1_000;
                for k in 0..whole {
                    finals.push(PalwExecFinalV1 {
                        domain: class.domain,
                        bond: class.bond,
                        operator_id: class.operator_id,
                        claim_id: crate::Hash64::from_u64_word((i as u64) << 40 | span << 16 | k),
                        execution_root: crate::Hash64::from_u64_word(span),
                        credit: 1,
                    });
                }
            }
            // This span's acceptances, through the gate.
            state.accepted_milli += class.accepted_per_span_milli;
            let inflight_milli: u64 = bound_queues[i].iter().map(|(_, a)| *a).sum();
            let cap_milli = (class.profile.max_inflight_claims as u64) * 1_000;
            let room = cap_milli.saturating_sub(inflight_milli);
            let bind_now = class.accepted_per_span_milli.min(room);
            let held = class.accepted_per_span_milli - bind_now;
            state.bound_milli += bind_now;
            state.held_milli += held;
            if bind_now > 0 {
                bound_queues[i].push((span + class.profile.verification_window_spans.max(1) as u64, bind_now));
            }
            state.inflight_milli = inflight_milli + bind_now;
            state.reserved_exposure_sompi = (state.inflight_milli as u128)
                .saturating_mul(class.profile.seat_count as u128)
                .saturating_mul(class.seat_exposure_sompi)
                / 1_000;
        }
    }
    PalwNetworkSimV1 {
        classes: states,
        schedule: palw_execution_schedule_snapshot_v1(spans + 2, &finals),
        anchor_period_ms: PALW_ANCHOR_PERIOD_MS_V1,
        spans,
    }
}

/// **The profile × artifact grid the operator asked for** (ADR-0133 §6): warm p99 60 / 120 / 240 /
/// 480 / 900 s against 30 / 100 / 300 GiB artifacts.
pub const PALW_SIM_WARM_P99_MS_V1: [u64; 5] = [60_000, 120_000, 240_000, 480_000, 900_000];
pub const PALW_SIM_ARTIFACT_GIB_V1: [u64; 3] = [30, 100, 300];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwVerificationSimRowV1 {
    pub warm_p99_ms: u64,
    pub artifact_gib: u64,
    pub profile: PalwVerificationProfileV1,
    pub capacity: PalwPanelCapacityV1,
}

/// The grid at one accepted rate, one seat pool, one cold fraction and one seat exposure.
pub fn palw_verification_sim_grid_v1(
    reference: &PalwVerificationReferenceV1,
    accepted_per_span_milli: u64,
    eligible_seats: u32,
    cold_fraction_permille: u32,
    seat_exposure_sompi: u128,
) -> Vec<PalwVerificationSimRowV1> {
    let mut rows = Vec::with_capacity(15);
    for &warm in &PALW_SIM_WARM_P99_MS_V1 {
        for &gib in &PALW_SIM_ARTIFACT_GIB_V1 {
            let artifact_bytes = gib << 30;
            let facts = PalwClassTimingFactsV1 {
                draw_compute: (warm as u128) * reference.mac_eq_per_ms as u128,
                artifact_bytes,
                warm_p99_ms: warm,
                cold_p99_ms: warm + artifact_bytes / reference.artifact_bytes_per_ms.max(1),
            };
            let profile = palw_verification_profile_v1(&facts, reference, PALW_SEAT_COUNT_V1);
            let capacity = palw_panel_capacity_v1(
                &PalwPanelCapacityInputV1 {
                    accepted_per_span_milli,
                    profile,
                    eligible_seats,
                    cold_fraction_permille,
                    seat_exposure_sompi,
                },
                reference,
            );
            rows.push(PalwVerificationSimRowV1 { warm_p99_ms: warm, artifact_gib: gib, profile, capacity });
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Hash64;
    use crate::palw_state_v2::PalwBondKeyV2;
    use crate::tx::{TransactionId, TransactionOutpoint};

    const R: PalwVerificationReferenceV1 = PALW_VERIFICATION_REFERENCE_V1;
    /// The dense tier as the fleet measured it: 83.1 G MAC-eq, a 0.8 GB artifact, 20 s warm.
    fn qwen25() -> PalwClassTimingFactsV1 {
        PalwClassTimingFactsV1 { draw_compute: 83_102_171_136, artifact_bytes: 800 << 20, warm_p99_ms: 20_100, cold_p99_ms: 0 }
    }
    /// The hybrid: 18.1 G MAC-eq, 24 GB, ~60 s warm (the operator's figure), 9.7 GiB read cold.
    fn qwen36() -> PalwClassTimingFactsV1 {
        PalwClassTimingFactsV1 { draw_compute: 18_055_200_736, artifact_bytes: 24 << 30, warm_p99_ms: 60_000, cold_p99_ms: 180_000 }
    }
    /// A Kimi-class stand-in: 1 T MAC-eq a draw, a 300 GiB artifact, 900 s warm.
    fn kimi() -> PalwClassTimingFactsV1 {
        PalwClassTimingFactsV1 { draw_compute: 1_000_000_000_000, artifact_bytes: 300 << 30, warm_p99_ms: 900_000, cold_p99_ms: 0 }
    }
    fn bond(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(n), 0))
    }
    fn class(n: u64, facts: PalwClassTimingFactsV1, rate_milli: u64, seats: u32, cold_permille: u32) -> PalwClassSimV1 {
        PalwClassSimV1 {
            domain: Hash64::from_u64_word(n),
            profile: palw_verification_profile_v1(&facts, &R, PALW_SEAT_COUNT_V1),
            accepted_per_span_milli: rate_milli,
            eligible_seats: seats,
            cold_fraction_permille: cold_permille,
            seat_exposure_sompi: 3 * 5 * 6_630_544,
            bond: bond(n),
            operator_id: Hash64::from_u64_word(100 + n),
        }
    }

    /// **The profile is derived, and a registrant cannot shrink it.** The dense tier's window is one
    /// span (2 × (20 s + 4 min) < 10 min); the hybrid's cold replay of three minutes needs two; a
    /// Kimi-class 900 s warm with 300 GiB cold reads (300 s at a gigabyte a second) needs
    /// 2 × (1,200 + 240) s = 2,880 s = five spans; under-reporting a p99 below the reference's
    /// estimate changes nothing.
    #[test]
    fn adr0133_the_profile_is_derived_and_cannot_be_shrunk() {
        let dense = palw_verification_profile_v1(&qwen25(), &R, 5);
        assert_eq!((dense.verification_window_spans, dense.artifact_prefetch_spans), (1, 1), "2 × (21.6 + 240) s < 600 s");
        assert_eq!(dense.warm_p99_ms, 20_775, "the estimate (83.1 G at 4 G/s) outranks the 20.1 s measured");
        assert_eq!(dense.cold_p99_ms, 20_775 + 838, "…plus 800 MiB at a gigabyte a second");
        let hybrid = palw_verification_profile_v1(&qwen36(), &R, 5);
        assert_eq!(hybrid.verification_window_spans, 2, "2 × (180 + 240) s = 840 s: two spans");
        assert_eq!(hybrid.artifact_prefetch_spans, 1, "24 GB at a gigabyte a second, doubled: 48 s");
        let heavy = palw_verification_profile_v1(&kimi(), &R, 5);
        assert_eq!(heavy.cold_p99_ms, 900_000 + 322_122, "900 s warm plus 300 GiB at a gigabyte a second");
        assert_eq!(heavy.verification_window_spans, 5, "2 × (900 + 322 + 240) s = 2,924 s: five spans");
        assert_eq!(heavy.artifact_prefetch_spans, 2, "300 GiB at a gigabyte a second, doubled: 644 s");
        // Under-reporting is floored at the reference's estimate: 1 T MAC-eq at 4 G/s is 250 s warm
        // however it is reported, 572 s cold with the 300 GiB read, three spans — and a measured p99
        // ABOVE the estimate (the 900 s above) widens the window, never narrows it.
        let lied = PalwClassTimingFactsV1 { warm_p99_ms: 1, cold_p99_ms: 1, ..kimi() };
        let from_lie = palw_verification_profile_v1(&lied, &R, 5);
        assert_eq!((from_lie.warm_p99_ms, from_lie.cold_p99_ms), (250_000, 250_000 + 322_122));
        assert_eq!(from_lie.verification_window_spans, 3, "2 × (572 + 240) s = 1,624 s: three spans, the floor");
        assert!(heavy.verification_window_spans > from_lie.verification_window_spans);
        // The inflight cap is Little's law at 70 % over seven seats: window / (5 × warm).
        assert_eq!(dense.max_inflight_claims, (700 * 7 * PALW_SPAN_MS_V1 / 1_000 / (5 * dense.warm_p99_ms)) as u32);
        assert!(heavy.max_inflight_claims >= 1);
    }

    /// **Little's law sizes the panel, and 60–70 % is where a panel still absorbs an outage.** At
    /// the dense tier's live rate (0.44 claims a DAA = 2.2 a span) with seven eligible seats the
    /// class runs at 5 % utilization (2.2 × 5 × 20.8 s against 7 × 600 s); at the same rate the
    /// Kimi-class stand-in needs 2.2 × 5 × 900 s = 9,900 seat-seconds a span against 4,200 —
    /// 236 %, dead — and 24 seats to sit at 70 %, 26 with two operators to spare. The rate the
    /// panel could carry at the target falls out of the same arithmetic.
    #[test]
    fn adr0133_littles_law_sizes_the_panel() {
        let dense = palw_panel_capacity_v1(
            &PalwPanelCapacityInputV1 {
                accepted_per_span_milli: 2_200,
                profile: palw_verification_profile_v1(&qwen25(), &R, 5),
                eligible_seats: 7,
                cold_fraction_permille: 0,
                seat_exposure_sompi: 3 * 5 * 6_630_544,
            },
            &R,
        );
        assert!((50..=60).contains(&dense.utilization_permille), "{}", dense.utilization_permille);
        assert_eq!(dense.outage_margin_seats, 2);
        assert!(dense.live && dense.within_target(&R));
        assert_eq!(dense.inflight_claims_milli, 2_200, "one span's claims in flight over a one-span window");
        assert_eq!(dense.final_latency_spans, 1 + 240, "the challenge window dominates: 1,200 anchors");
        let heavy = palw_panel_capacity_v1(
            &PalwPanelCapacityInputV1 {
                accepted_per_span_milli: 2_200,
                profile: palw_verification_profile_v1(&kimi(), &R, 5),
                eligible_seats: 7,
                cold_fraction_permille: 0,
                seat_exposure_sompi: 3 * 5 * 6_630_544,
            },
            &R,
        );
        assert!(heavy.utilization_permille >= 2_300 && !heavy.live, "{}", heavy.utilization_permille);
        assert_eq!(heavy.seats_required, 24, "9,900 seat-seconds a span at 70 % of 600 s");
        assert_eq!((heavy.seats_required_n1, heavy.seats_required_n2), (25, 26));
        assert!(
            heavy.max_accepted_per_span_milli < 2_200 && heavy.max_accepted_per_span_milli > 600,
            "{}",
            heavy.max_accepted_per_span_milli
        );
        assert!(heavy.reserved_exposure_sompi > dense.reserved_exposure_sompi, "five spans in flight hold five times the collateral");
        // Cold replays: the same rate at a 50 % cold fraction costs more seat-time.
        let half_cold = palw_panel_capacity_v1(
            &PalwPanelCapacityInputV1 {
                accepted_per_span_milli: 2_200,
                profile: palw_verification_profile_v1(&qwen36(), &R, 5),
                eligible_seats: 7,
                cold_fraction_permille: 500,
                seat_exposure_sompi: 1,
            },
            &R,
        );
        let warm = palw_panel_capacity_v1(
            &PalwPanelCapacityInputV1 {
                accepted_per_span_milli: 2_200,
                profile: palw_verification_profile_v1(&qwen36(), &R, 5),
                eligible_seats: 7,
                cold_fraction_permille: 0,
                seat_exposure_sompi: 1,
            },
            &R,
        );
        assert!(half_cold.service_ms > warm.service_ms && half_cold.utilization_permille > warm.utilization_permille);
    }

    /// **The gate holds a class's own claims and no other's.**
    #[test]
    fn adr0133_the_gate_is_class_local() {
        assert_eq!(palw_class_gate_v1(0, 4), PalwClassGateV1::Bind);
        assert_eq!(palw_class_gate_v1(3, 4), PalwClassGateV1::Bind);
        assert_eq!(palw_class_gate_v1(4, 4), PalwClassGateV1::Hold);
        assert_eq!(palw_class_gate_v1(9, 4), PalwClassGateV1::Hold);
        assert_eq!(palw_class_gate_v1(u32::MAX, 0), PalwClassGateV1::Bind, "no cap: no gate");
    }

    /// **The property the operator named, as a property: a Kimi-class starvation stops only
    /// Kimi.** Over a grid of rates, seat pools, caches and artifact sizes — Qwen only, Kimi only,
    /// both, a one-span class beside a five-span one, the slow class at 90 %, the fast class at
    /// 90 %, one and two operators down, cold and warm caches, 100 and 300 GiB — Qwen's every
    /// count is IDENTICAL with Kimi in the network and without it; Kimi's claims are all held or
    /// bound, licensed exactly when its own panel is live by its own arithmetic, and never
    /// counted anywhere else; the lane schedules exactly the classes with a `Final`; the anchor
    /// cadence is the constant it was. Kimi recovers when its seats return; a cap holds the surplus.
    #[test]
    fn adr0133_a_kimi_starvation_stops_only_kimi() {
        let spans = 300;
        let mut cases = 0;
        let mut kimi_live_cases = 0;
        for (qwen_rate, kimi_rate) in [(2_200u64, 0u64), (0, 2_200), (2_200, 2_200), (1_100, 1_100), (200, 1_800), (1_800, 200)] {
            for (qwen_seats, kimi_seats) in [(7u32, 7u32), (6, 6), (5, 5), (7, 0), (7, 2)] {
                for cold in [0u32, 1_000] {
                    for gib in [100u64, 300u64] {
                        let heavy_facts = PalwClassTimingFactsV1 { artifact_bytes: gib << 30, ..kimi() };
                        let qwen = class(1, qwen25(), qwen_rate, qwen_seats, cold);
                        let heavy = class(2, heavy_facts, kimi_rate, kimi_seats, cold);
                        let alone = palw_network_sim_v1(std::slice::from_ref(&qwen), spans, &R);
                        let mixed = palw_network_sim_v1(&[qwen.clone(), heavy.clone()], spans, &R);
                        let (q, k) = (mixed.classes[0], mixed.classes[1]);
                        cases += 1;
                        assert_eq!(mixed.anchor_period_ms, PALW_ANCHOR_PERIOD_MS_V1, "no class touches the anchor cadence");
                        assert_eq!(q, alone.classes[0], "Qwen's every count is the same with Kimi in the network and without");
                        let qwen_domain = Hash64::from_u64_word(1);
                        assert_eq!(
                            mixed.domain(&qwen_domain).map(|d| d.credits),
                            alone.domain(&qwen_domain).map(|d| d.credits),
                            "Qwen's lane credits are the same with Kimi and without"
                        );
                        assert_eq!(k.held_milli + k.bound_milli, k.accepted_milli, "every Kimi claim is held or bound, none lost");
                        let kimi_capacity = palw_panel_capacity_v1(
                            &PalwPanelCapacityInputV1 {
                                accepted_per_span_milli: kimi_rate,
                                profile: heavy.profile,
                                eligible_seats: kimi_seats,
                                cold_fraction_permille: cold,
                                seat_exposure_sompi: heavy.seat_exposure_sompi,
                            },
                            &R,
                        );
                        assert_eq!(k.live, kimi_capacity.live);
                        if kimi_rate > 0 {
                            if kimi_capacity.live {
                                kimi_live_cases += 1;
                                assert!(k.licensed_milli > 0 && k.finals_milli > 0, "a live Kimi panel licenses: {k:?}");
                            } else {
                                assert_eq!((k.licensed_milli, k.finals_milli), (0, 0), "nothing licenses on a dead panel: {k:?}");
                            }
                        }
                        if qwen_rate > 0 {
                            assert!(q.live, "Qwen's panel of {qwen_seats} seats at {qwen_rate}‰ a span is live");
                            assert_eq!((q.held_milli, q.voided_milli), (0, 0), "Qwen is never held or voided for Kimi");
                            assert!(q.finals_milli > 0, "Qwen finals arrive after the challenge window");
                            assert!(mixed.domain(&qwen_domain).is_some(), "the lane schedules Qwen's domain");
                        }
                        let kimi_domain = Hash64::from_u64_word(2);
                        assert_eq!(
                            mixed.domain(&kimi_domain).is_some(),
                            k.finals_milli >= 1_000,
                            "a class holds permits exactly when it has a Final"
                        );
                        if qwen_rate == 0 && !kimi_capacity.live {
                            assert!(
                                mixed.schedule.domains.is_empty(),
                                "Kimi alone and starved schedules nothing — and the chain's anchors still come"
                            );
                        }
                    }
                }
            }
        }
        assert_eq!(cases, 6 * 5 * 2 * 2);
        assert!(kimi_live_cases > 0, "the grid holds live Kimi cases too (a low rate on a full panel)");
        // Recovery: the same Kimi class with the seats its profile needs is live and licenses.
        let recovered = class(2, kimi(), 2_200, 26, 0);
        let net = palw_network_sim_v1(&[class(1, qwen25(), 2_200, 7, 0), recovered.clone()], spans, &R);
        let k = net.classes[1];
        assert!(k.live && k.licensed_milli > 0 && k.finals_milli > 0, "{k:?}");
        assert!(net.domain(&Hash64::from_u64_word(2)).is_some(), "…and holds permits once it has Finals");
        // The inflight cap holds the surplus: a class at many times its cap's rate binds the cap and holds the rest.
        let capped =
            PalwClassSimV1 { profile: PalwVerificationProfileV1 { max_inflight_claims: 2, ..recovered.profile }, ..recovered };
        let net = palw_network_sim_v1(&[capped], 20, &R);
        assert!(net.classes[0].held_milli > 0 && net.classes[0].inflight_milli <= 2_000, "{:?}", net.classes[0]);
    }

    /// **Collateral is the other queue.** Over a five-span window a Kimi-class panel holds
    /// `inflight × seats × exposure`; at the ADR-0130 λ = 2 exposure (256 MSK a seat) a 10,000 MSK
    /// bond carries 39 duties, so the class's inflight cap and its rate are bounded by the seats'
    /// free collateral before they are bounded by time — the arithmetic here says by how much.
    #[test]
    fn adr0133_collateral_bounds_the_inflight_before_time_does() {
        let lambda_exposure: u128 = 256 * 100_000_000;
        let bond_collateral: u128 = 10_000 * 100_000_000;
        let heavy = palw_panel_capacity_v1(
            &PalwPanelCapacityInputV1 {
                accepted_per_span_milli: 2_200,
                profile: palw_verification_profile_v1(&kimi(), &R, 5),
                eligible_seats: 26,
                cold_fraction_permille: 0,
                seat_exposure_sompi: lambda_exposure,
            },
            &R,
        );
        assert!(heavy.live);
        let duties_per_seat = heavy.reserved_exposure_sompi / lambda_exposure / 26;
        assert!(duties_per_seat <= 39, "{duties_per_seat} duties a seat hold under a 10,000 MSK bond at λ = 2");
        let bonds_needed = heavy.reserved_exposure_sompi.div_ceil(bond_collateral);
        assert!((1..=26).contains(&bonds_needed), "{bonds_needed}");
        // Exhaustion: at two hundred times that exposure (a seat reward of 25,600 MSK under λ = 2)
        // the inflight duties need more bonds than there are seats — the gate must hold before the
        // collateral runs out, and the profile's cap is what holds it.
        let exhausted = palw_panel_capacity_v1(
            &PalwPanelCapacityInputV1 {
                accepted_per_span_milli: 2_200,
                profile: palw_verification_profile_v1(&kimi(), &R, 5),
                eligible_seats: 26,
                cold_fraction_permille: 0,
                seat_exposure_sompi: lambda_exposure * 200,
            },
            &R,
        );
        assert!(exhausted.reserved_exposure_sompi.div_ceil(bond_collateral) > 26);
    }

    /// **The grid**: windows grow with the p99 and the artifact, prefetch with the artifact alone,
    /// utilization with the service time; the 60-second profile at 30 GiB is one span and the
    /// 900-second profile at 300 GiB is five.
    #[test]
    fn adr0133_the_profile_grid_is_monotone() {
        let rows = palw_verification_sim_grid_v1(&R, 2_200, 7, 0, 1);
        assert_eq!(rows.len(), 15);
        assert_eq!(
            rows[0].profile.verification_window_spans, 2,
            "60 s warm, 30 GiB cold (32 s), 240 s of receipt: 2 × 332 s > one span"
        );
        assert_eq!(rows[14].profile.verification_window_spans, 5, "900 s warm, 300 GiB cold (322 s): 2 × 1,462 s = five spans");
        for w in rows.windows(2) {
            let (a, b) = (w[0], w[1]);
            if a.warm_p99_ms == b.warm_p99_ms {
                assert!(
                    b.profile.cold_p99_ms > a.profile.cold_p99_ms
                        && b.profile.artifact_prefetch_spans >= a.profile.artifact_prefetch_spans
                );
                assert!(b.profile.verification_window_spans >= a.profile.verification_window_spans);
            } else {
                assert!(
                    b.profile.warm_p99_ms > a.profile.warm_p99_ms && b.capacity.utilization_permille > a.capacity.utilization_permille
                );
            }
        }
        assert!(rows.iter().filter(|r| r.capacity.live).count() < rows.len(), "seven seats do not carry the heavy end of the grid");
    }

    /// **The grid's numbers, pinned as ADR-0133 §6 prints them** (2.2 claims a span, seven seats, warm):
    /// windows 2/2/3 · 2/2/3 · 2/2/3 · 3/3/4 · 4/5/5 spans over the three artifacts; prefetch 1/1/2;
    /// utilization 157 / 314 / 628 / 1,257 / 2,357 ‰; seats at 70 % 5 / 5 / 7 / 13 / 24; the inflight cap
    /// `⌊0.98 × window / warm⌋`; throughput at 70 % 9.8 / 4.9 / 2.45 / 1.22 / 0.65 claims a span.
    #[test]
    fn adr0133_the_grid_is_what_the_adr_prints() {
        let rows = palw_verification_sim_grid_v1(&R, 2_200, 7, 0, 1);
        let windows: Vec<u32> = rows.iter().map(|r| r.profile.verification_window_spans).collect();
        assert_eq!(windows, vec![2, 2, 3, 2, 2, 3, 2, 2, 3, 3, 3, 4, 4, 5, 5]);
        let prefetch: Vec<u32> = rows.iter().map(|r| r.profile.artifact_prefetch_spans).collect();
        assert_eq!(prefetch, vec![1, 1, 2, 1, 1, 2, 1, 1, 2, 1, 1, 2, 1, 1, 2]);
        let caps: Vec<u32> = rows.iter().map(|r| r.profile.max_inflight_claims).collect();
        assert_eq!(caps, vec![19, 19, 29, 9, 9, 14, 4, 4, 7, 3, 3, 4, 2, 3, 3]);
        let util: Vec<u32> = rows.iter().step_by(3).map(|r| r.capacity.utilization_permille).collect();
        assert_eq!(util, vec![157, 314, 628, 1_257, 2_357]);
        let seats: Vec<u32> = rows.iter().step_by(3).map(|r| r.capacity.seats_required).collect();
        assert_eq!(seats, vec![5, 5, 7, 13, 24]);
        let throughput: Vec<u64> = rows.iter().step_by(3).map(|r| r.capacity.max_accepted_per_span_milli).collect();
        assert_eq!(throughput, vec![9_800, 4_900, 2_450, 1_225, 653]);
        let live: Vec<bool> = rows.iter().step_by(3).map(|r| r.capacity.live).collect();
        assert_eq!(live, vec![true, true, true, false, false]);
        assert!(rows.iter().all(|r| r.capacity.final_latency_spans == r.profile.verification_window_spans as u64 + 240));
    }
}
