//! **G14-B: maximum-profile budgets and deadlines — can a public prosecution FINISH?** (lane G14-B, 2026-10-11;
//! `docs/design/palw/g14-completion-matrix.md` §2c).
//!
//! [`crate::gate`] bounds what ONE prosecution reads, opens and holds (bytes, court work, RAM, retained state, sessions). It does not
//! bound WHEN it ends. This module derives that, as code, from the route's carriage and the network's clock:
//!
//! * **Carriage.** Every route object larger than one carrier rides the route's own chunk lane (tag 113, `KernelRouteChunkV1`): chunks
//!   of at most [`ROUTE_CHUNK_BYTES_V1`] bytes, at most [`ROUTE_CHUNKS_PER_OBJECT_V1`] of them, around a wrapper of at most
//!   [`ROUTE_OBJECT_WRAPPER_BYTES_V1`]. A block carries at most `max_block_mass / TRANSIENT_BYTE_TO_MASS_FACTOR` = 125,000 bytes of
//!   transactions, so at most ONE full chunk a block: [`ROUTE_DAA_PER_CHUNK_V1`] = 1 (the consumer pins these mirrors against its own
//!   constants).
//! * **Chunk-counting response windows.** A demand's response window is the base court deadline (the producer's reaction and ONE
//!   carrier) plus one DAA for every further chunk the demanded material needs, counted from the class's deterministic layout — never
//!   from what the producer sends, so a maximal answer cannot buy time ([`response_window_daa_v1`]).
//! * **The pursuit schedule.** A segmented prosecution is sequential: up to `⌈log2 min(P, 1024)⌉` adaptive position probes and one final
//!   round (G14-V's search; [`seg_search_rounds_v1`] is its round count, consumed as a parameter), each demand answered at the LAST DAA
//!   of its window by a stalling producer, then the filing. [`seg_pursuit_schedule_v1`] runs that worst case against the lifecycle (the
//!   window, the dead zone between a served demand and Final, the post-Final admission rule `daa + window + grace ≤ liability_until`)
//!   and returns the least liability horizon under which it completes.
//! * **The class's liability.** A class whose pursuit needs more than the policy's `liability_daa` gets exactly what it needs — its
//!   claims stay liable that much longer — as long as the claim's whole worst-case LIFETIME (commitment to the end of liability) stays
//!   within a ceiling; past it the class is REFUSED at registration by name ([`class_pursuit_v1`]). ECON's `W ≥ L` (L the longest
//!   claim lifetime) makes `W`'s top, [`LIFETIME_CEILING_DAA_V1`] = 436, the most any admissible `W` can carry. `W` is not decided
//!   here: a class with lifetime `L` needs `W ≥ L`; [`class_pursuit_with_ceiling_v1`] takes any ceiling and the tests sweep the range.
//! * **Sessions** ([`session_pursuit_schedule_v1`]): the legacy route's reserved DA sessions (one open at a time, each answered at its
//!   deadline) against the claim's hard deadline.
//! * **Population** ([`concurrent_pursuits_bound_v1`]): the chain carries one chunk a DAA, whoever's.
//!
//! Every number here is derived from consensus constants and policy terms. Nothing is measured: verifier compute time enters only
//! through the OPV policy's declared budgets, which are claims an external gate must measure.

use crate::descriptor::KernelDescriptorV1;
use crate::gate::ProsecutionGapV1;
use crate::ledger::LedgerPolicyV1;
use crate::opv::OpvPolicyV1;
use crate::plan::VerificationPlanV1;
use misaka_palw_tir::program::TirProgramV1;

/// One route chunk's payload (mirrors the consumer's `PALW_OBJECT_CHUNK_MAX_BYTES`).
pub const ROUTE_CHUNK_BYTES_V1: u64 = 100_000;
/// The most chunks one route object spans (mirrors `PALW_OBJECT_CHUNK_MAX_COUNT`).
pub const ROUTE_CHUNKS_PER_OBJECT_V1: u64 = 16;
/// The consumer's allowance for what wraps a kernel object on its carrier: the outer object tag, the signer, the ML-DSA-87 signature
/// and the length prefixes (mirrors the `16 << 10` in `PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1`).
pub const ROUTE_OBJECT_WRAPPER_BYTES_V1: u64 = 16 << 10;
/// The longest a chunk group stays half-assembled (mirrors `PALW_KERNEL_CHUNK_TTL_MAX_DAA_V1`).
pub const ROUTE_CHUNK_TTL_MAX_DAA_V1: u64 = 64;
/// **DAA per chunk.** A block carries at most `max_block_mass (500,000) / TRANSIENT_BYTE_TO_MASS_FACTOR (4)` = 125,000 transaction
/// bytes, and a full chunk's carrier is more than 100,000: one full chunk a block, one block a DAA.
pub const ROUTE_DAA_PER_CHUNK_V1: u64 = 1;
/// `Respond`'s envelope around the served bytes (version, tag, claim, stage, position, length) — the ledger's `RESPOND_OVERHEAD_V1`.
pub const RESPOND_ENVELOPE_BYTES_V1: u64 = 1 + 1 + 64 + 1 + 4 + 4;
/// `FileProof`'s envelope around the filing (version, tag, accuser, claim, variant, length) — the ledger's `FILE_PROOF_OVERHEAD_V1`.
pub const FILE_PROOF_ENVELOPE_BYTES_V1: u64 = 1 + 1 + 64 + 64 + 1 + 4;
/// **The registration ceiling on a claim's worst-case lifetime** (commitment → end of liability, DAA): the top of `W`'s validated range
/// (mirrors `PALW_BOND_BUDGET_W_MAX_DAA_V1`, 436). ECON derives `W ≥ L` for the longest claim lifetime `L`, so a class whose lifetime
/// exceeds 436 can be carried by no admissible `W`; one within needs the chosen `W` to be at least its lifetime (reported, not decided
/// here: [`class_pursuit_with_ceiling_v1`] takes any ceiling). The interim kernel lifetime (window 50 + court 20 + grace 10 +
/// liability 200 = 280) is ECON's `L = 280`, the bottom of the range.
pub const LIFETIME_CEILING_DAA_V1: u64 = 436;

/// **The route chunks an object of `payload` encoded bytes takes** (at least one; its wrapper counted).
pub fn route_object_chunks_v1(payload: u128) -> u64 {
    let total = payload.saturating_add(ROUTE_OBJECT_WRAPPER_BYTES_V1 as u128);
    total.div_ceil(ROUTE_CHUNK_BYTES_V1 as u128).clamp(1, u64::MAX as u128) as u64
}

/// Whether one object of `payload` bytes can ride the route at all: within the chunk count, and assembled inside a group's TTL.
pub fn route_object_carriable_v1(payload: u128) -> bool {
    let chunks = route_object_chunks_v1(payload);
    chunks <= ROUTE_CHUNKS_PER_OBJECT_V1 && chunks.saturating_mul(ROUTE_DAA_PER_CHUNK_V1) <= ROUTE_CHUNK_TTL_MAX_DAA_V1
}

/// **The chunk-counting response window**: the base court deadline covers the producer's reaction and one carrier; every further chunk
/// adds [`ROUTE_DAA_PER_CHUNK_V1`]. `chunks` is the DECLARED material's count (the class layout), never the producer's choice.
pub fn response_window_daa_v1(court_deadline_daa: u64, chunks: u64) -> u64 {
    court_deadline_daa.saturating_add(chunks.saturating_sub(1).saturating_mul(ROUTE_DAA_PER_CHUNK_V1))
}

/// **The route chunks serving segmented position `p` takes**: every part is one `Respond`, chunked on its own (a part never shares a
/// chunk with another). `(chunks, parts)`.
pub fn seg_position_chunks_v1(program: &TirProgramV1, p: u32) -> Result<(u64, u32), String> {
    let parts = crate::seg_da::position_part_bytes_v1(program, p)?;
    let chunks = parts.iter().fold(0u64, |sum, b| {
        sum.saturating_add(route_object_chunks_v1((*b as u128).saturating_add(RESPOND_ENVELOPE_BYTES_V1 as u128)))
    });
    Ok((chunks, parts.len() as u32))
}

/// The chunks of an inline (K2-TIR v1–v3, typed) position response of at most `max_response_bytes` — capped at one object: a class
/// whose response exceeds one object is refused by the consumer's carrier fit (a demand on it could never be served).
pub fn inline_response_chunks_v1(max_response_bytes: u128) -> u64 {
    route_object_chunks_v1(max_response_bytes.saturating_add(RESPOND_ENVELOPE_BYTES_V1 as u128)).min(ROUTE_CHUNKS_PER_OBJECT_V1)
}

/// **G14-V's search, as a round count** (`crate::seg_detect`): the descent of the first divergent segment takes at most
/// `⌈log2 S⌉` probes (`S = min(P, SEG_LEN_V4)` positions in a segment), each a full position demand, and the court's two positions
/// `q − 1`, `q` take one more round (demanded together: a demander holds four open sessions). One position (an encoder): one round.
pub fn seg_search_rounds_v1(positions: u32) -> u32 {
    let s = positions.clamp(1, crate::seg::SEG_LEN_V4) as u64;
    crate::merkle::depth(s) as u32 + 1
}

/// **The clock a fresh public verifier runs on**, in DAA (RFC-0015 §6.1's budgets; the user's 2026-10-08 ruling
/// `T_challenge ≥ T_beacon + T_fetch + T_check + T_localize + T_file + T_margin`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PursuitClockV1 {
    /// The claim's fixed challenge window from its commitment (OPV: base + horizon). No pre-Final demand opens past it.
    pub window_daa: u64,
    pub court_deadline_daa: u64,
    pub proof_grace_daa: u64,
    pub liability_daa: u64,
    /// Cold fetch + check + one inclusion + reorg slack: the commitment to the first demand.
    pub first_step_daa: u64,
    /// One probe's turn: read the served path, localize one level further, have the next demand included.
    pub turnaround_daa: u64,
    /// Localize + court assembly + inclusion of the filing's first chunk + reorg slack (the OPV `prove` path).
    pub prove_daa: u64,
}

impl PursuitClockV1 {
    /// The OPV route's clock from its policies (the declared budgets — claims about the network that an external gate measures).
    pub fn of_opv(l: &LedgerPolicyV1, p: &OpvPolicyV1) -> Self {
        let b = &p.budgets;
        Self {
            window_daa: p.window_daa(),
            court_deadline_daa: l.court_deadline_daa,
            proof_grace_daa: l.proof_grace_daa,
            liability_daa: l.liability_daa,
            first_step_daa: b
                .cold_material_daa
                .saturating_add(b.check_daa)
                .saturating_add(b.carrier_daa)
                .saturating_add(b.reorg_slack_daa),
            turnaround_daa: b.localize_daa.saturating_add(b.carrier_daa).max(1),
            prove_daa: b.localize_daa.saturating_add(b.court_daa).saturating_add(b.carrier_daa).saturating_add(b.reorg_slack_daa),
        }
    }
}

/// **The worst-case schedule of one sequential pursuit**, DAA relative to the claim's commitment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PursuitScheduleV1 {
    pub rounds: u32,
    /// Rounds whose demand opened before the window closed (each holds Final until its service and grace).
    pub pre_final_rounds: u32,
    /// The earliest the producer can reach Final under this pursuit.
    pub final_daa: u64,
    pub last_demand_daa: u64,
    pub last_served_daa: u64,
    /// The filing's last chunk lands.
    pub filed_daa: u64,
    /// **The least `liability_daa` under which every post-Final demand is admitted and the filing lands** (0: all before Final).
    pub liability_needed_daa: u64,
}

impl PursuitScheduleV1 {
    pub fn fits(&self, liability_daa: u64) -> bool {
        self.liability_needed_daa <= liability_daa
    }
}

/// **Run the worst case.** `rounds` sequential demands, each answered at the last DAA of `window_per_round` (the producer stalls), then a
/// filing of `filing_chunks` chunks. The claim reaches Final as early as the lifecycle allows: at the window's end, or once the last
/// pre-Final demand is served and its grace has run (`final_hold_until`). Between a served pre-Final demand and Final no demand opens
/// (`WindowClosed`); after Final a demand opens only if its whole path fits (`daa + window + grace ≤ liability_until`).
pub fn seg_pursuit_schedule_v1(clock: &PursuitClockV1, rounds: u32, window_per_round: u64, filing_chunks: u64) -> PursuitScheduleV1 {
    let (w_end, w, grace) = (clock.window_daa, window_per_round, clock.proof_grace_daa);
    let mut hold = 0u64;
    let mut t = clock.first_step_daa;
    let (mut pre, mut need, mut last, mut served) = (0u32, 0u64, 0u64, 0u64);
    let mut final_daa: Option<u64> = None;
    for _ in 0..rounds {
        if final_daa.is_none() && t < w_end {
            pre += 1;
            served = t.saturating_add(w);
            hold = hold.max(served.saturating_add(grace));
        } else {
            let f = *final_daa.get_or_insert(w_end.max(hold));
            t = t.max(f.saturating_add(1)); // Final happens in block f's tick, after its objects: the next block is the first post-Final one
            need = need.max(t.saturating_add(w).saturating_add(grace) - f);
            served = t.saturating_add(w);
        }
        last = t;
        t = served.saturating_add(clock.turnaround_daa);
    }
    let f = final_daa.unwrap_or(w_end.max(hold));
    let filed =
        served.saturating_add(clock.prove_daa).saturating_add(filing_chunks.saturating_sub(1).saturating_mul(ROUTE_DAA_PER_CHUNK_V1));
    if filed > f {
        need = need.max(filed - f);
    }
    PursuitScheduleV1 {
        rounds,
        pre_final_rounds: pre,
        final_daa: f,
        last_demand_daa: last,
        last_served_daa: served,
        filed_daa: filed,
        liability_needed_daa: need,
    }
}

/// **A class's pursuit, derived** — what registration stores and every Final reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassPursuitV1 {
    pub rounds: u32,
    /// The chunks of the worst demanded material (the last position of a segmented class; one inline response otherwise).
    pub response_chunks: u64,
    pub response_window_daa: u64,
    pub filing_chunks: u64,
    pub schedule: PursuitScheduleV1,
    /// **The claim's liability horizon**: `max(policy liability_daa, the schedule's need)`.
    pub liability_daa: u64,
    /// The claim's worst-case lifetime: the earliest Final under the pursuit plus the liability horizon. The chosen `W` must cover it.
    pub lifetime_daa: u64,
}

/// [`class_pursuit_with_ceiling_v1`] at the registration ceiling ([`LIFETIME_CEILING_DAA_V1`]).
pub fn class_pursuit_v1(
    descriptor: &KernelDescriptorV1,
    program: &TirProgramV1,
    plan: &VerificationPlanV1,
    max_filing_bytes: u64,
    max_response_bytes: u128,
    ledger: &LedgerPolicyV1,
    opv: &OpvPolicyV1,
) -> Result<ClassPursuitV1, ProsecutionGapV1> {
    class_pursuit_with_ceiling_v1(
        descriptor,
        program,
        plan,
        max_filing_bytes,
        max_response_bytes,
        ledger,
        opv,
        LIFETIME_CEILING_DAA_V1,
    )
}

/// **The schedule gate of a class on the OPV route**: its search rounds, the chunk-counted window of its worst demanded material, its
/// filing's chunks, and the worst-case schedule. `Err` (by name) when the filing cannot ride the route or the claim's worst-case
/// lifetime (commitment → end of its liability) exceeds `ceiling`.
#[allow(clippy::too_many_arguments)]
pub fn class_pursuit_with_ceiling_v1(
    descriptor: &KernelDescriptorV1,
    program: &TirProgramV1,
    plan: &VerificationPlanV1,
    max_filing_bytes: u64,
    max_response_bytes: u128,
    ledger: &LedgerPolicyV1,
    opv: &OpvPolicyV1,
    ceiling: u64,
) -> Result<ClassPursuitV1, ProsecutionGapV1> {
    let gap = |what: &'static str, required: u128, limit: u128| ProsecutionGapV1::Unbounded { what, required, limit };
    let filing = (max_filing_bytes as u128).saturating_add(FILE_PROOF_ENVELOPE_BYTES_V1 as u128);
    let filing_chunks = route_object_chunks_v1(filing);
    if !route_object_carriable_v1(filing) {
        return Err(gap(
            "PURSUIT_SCHEDULE: the worst filing does not ride the route's chunk lane (chunks)",
            filing_chunks as u128,
            ROUTE_CHUNKS_PER_OBJECT_V1 as u128,
        ));
    }
    let (rounds, response_chunks) = if crate::descriptor::is_segmented_v1(descriptor) {
        let last = plan.max_positions.saturating_sub(1);
        let (chunks, _) =
            seg_position_chunks_v1(program, last).map_err(|_| gap("PURSUIT_SCHEDULE: the last position does not lay out", 1, 0))?;
        (seg_search_rounds_v1(plan.max_positions), chunks)
    } else {
        // One round: every position demanded at once, each one object (the consumer's carrier fit refuses larger).
        (1, inline_response_chunks_v1(max_response_bytes))
    };
    let clock = PursuitClockV1::of_opv(ledger, opv);
    let window = response_window_daa_v1(ledger.court_deadline_daa, response_chunks);
    let schedule = seg_pursuit_schedule_v1(&clock, rounds, window, filing_chunks);
    let liability_daa = ledger.liability_daa.max(schedule.liability_needed_daa);
    let lifetime = schedule.final_daa.saturating_add(liability_daa);
    if lifetime > ceiling {
        return Err(gap(
            "PURSUIT_SCHEDULE: the worst-case pursuit (search rounds x chunk-counted response windows + filing) needs a claim \
             lifetime (commitment to the end of liability, DAA) past the ceiling",
            lifetime as u128,
            ceiling as u128,
        ));
    }
    Ok(ClassPursuitV1 {
        rounds,
        response_chunks,
        response_window_daa: window,
        filing_chunks,
        schedule,
        liability_daa,
        lifetime_daa: lifetime,
    })
}

/// **The legacy route's reserved sessions** (LG14-A/B): one open at a time per reserver (DA-1), each answered at the last DAA of its
/// window, opened while `daa ≤ open_span_daa` (the claim's hard deadline, `trace_retention − W_disclose`, from the first DAA the
/// verifier can act), and the filing landing by `close_span_daa` (the retention obligation's end).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionClockV1 {
    pub open_span_daa: u64,
    pub close_span_daa: u64,
    /// A session's answer window (DA: `now + W_disclose`).
    pub session_window_daa: u64,
    /// The reservation's inclusion and the verifier's own replay before the first session.
    pub first_step_daa: u64,
    pub turnaround_daa: u64,
    pub prove_daa: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionScheduleV1 {
    pub sessions: u32,
    pub last_open_daa: u64,
    pub filed_daa: u64,
    pub fits: bool,
    /// The most sessions that fit this clock.
    pub max_sessions: u32,
}

/// **Run the legacy worst case** for `sessions` sequential sessions.
pub fn session_pursuit_schedule_v1(c: &SessionClockV1, sessions: u32) -> SessionScheduleV1 {
    let cycle = c.session_window_daa.saturating_add(c.turnaround_daa).max(1);
    let open_k = |k: u32| c.first_step_daa.saturating_add((k.saturating_sub(1) as u64).saturating_mul(cycle));
    let filed_k = |k: u32| open_k(k).saturating_add(c.session_window_daa).saturating_add(c.prove_daa);
    let fits_k = |k: u32| k == 0 || (open_k(k) <= c.open_span_daa && filed_k(k) <= c.close_span_daa);
    // The largest k that fits (monotone in k).
    let (mut lo, mut hi) = (0u32, 1u32 << 20);
    while lo < hi {
        let mid = lo + (hi - lo).div_ceil(2);
        if fits_k(mid) { lo = mid } else { hi = mid - 1 }
    }
    SessionScheduleV1 {
        sessions,
        last_open_daa: open_k(sessions),
        filed_daa: filed_k(sessions),
        fits: fits_k(sessions),
        max_sessions: lo,
    }
}

/// **Population**: the most pursuits of `chunks_per_pursuit` route chunks each the chain can carry inside `span_daa` (one chunk a
/// DAA, whoever's). A bound on what the chain CAN carry, not on what inclusion order a miner chooses.
pub fn concurrent_pursuits_bound_v1(span_daa: u64, chunks_per_pursuit: u64) -> u64 {
    span_daa / chunks_per_pursuit.max(1).saturating_mul(ROUTE_DAA_PER_CHUNK_V1).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn interim_clock(liability: u64) -> PursuitClockV1 {
        // The interim OPV budgets (cold 10, check 10, localize 2, court 3, carrier 2, reorg 2) and windows (50, 20, 10).
        PursuitClockV1 {
            window_daa: 50,
            court_deadline_daa: 20,
            proof_grace_daa: 10,
            liability_daa: liability,
            first_step_daa: 24,
            turnaround_daa: 4,
            prove_daa: 9,
        }
    }

    #[test]
    fn chunks_count_the_wrapper_and_stop_at_the_object_ceiling() {
        let cap = ROUTE_CHUNK_BYTES_V1 as u128 * ROUTE_CHUNKS_PER_OBJECT_V1 as u128 - ROUTE_OBJECT_WRAPPER_BYTES_V1 as u128;
        assert_eq!(route_object_chunks_v1(0), 1);
        assert_eq!(route_object_chunks_v1(ROUTE_CHUNK_BYTES_V1 as u128 - ROUTE_OBJECT_WRAPPER_BYTES_V1 as u128), 1);
        assert_eq!(route_object_chunks_v1(ROUTE_CHUNK_BYTES_V1 as u128 - ROUTE_OBJECT_WRAPPER_BYTES_V1 as u128 + 1), 2);
        // Exactly at the ceiling: carriable; one byte over: not.
        assert_eq!(route_object_chunks_v1(cap), ROUTE_CHUNKS_PER_OBJECT_V1);
        assert!(route_object_carriable_v1(cap));
        assert!(!route_object_carriable_v1(cap + 1));
        // A maximal part (1 MiB) in its Respond: eleven chunks.
        let part = crate::seg_da::SEG_PART_BYTES_V4 as u128 + RESPOND_ENVELOPE_BYTES_V1 as u128;
        assert_eq!(route_object_chunks_v1(part), 11);
        assert_eq!(response_window_daa_v1(20, 11), 30);
        assert_eq!(response_window_daa_v1(20, 1), 20, "one carrier: the base deadline, unchanged");
        assert_eq!(response_window_daa_v1(20, 0), 20);
    }

    #[test]
    fn search_rounds_follow_the_segment_depth() {
        assert_eq!(seg_search_rounds_v1(1), 1);
        assert_eq!(seg_search_rounds_v1(2), 2);
        assert_eq!(seg_search_rounds_v1(1024), 11);
        assert_eq!(seg_search_rounds_v1(1025), 11, "the descent is inside one segment");
        assert_eq!(seg_search_rounds_v1(8192), 11);
    }

    /// The interim clock carries the deepest search only with a liability past 200: the boundary, exactly.
    #[test]
    fn the_interim_clock_needs_a_longer_liability_for_the_deepest_search() {
        let s = seg_pursuit_schedule_v1(&interim_clock(200), 11, 20, 1);
        assert_eq!(s.pre_final_rounds, 2, "two probes open before the window closes and hold Final");
        assert_eq!(s.final_daa, 78);
        assert_eq!(s.liability_needed_daa, 223);
        assert!(!s.fits(222) && s.fits(223) && !s.fits(200));
        // One round fewer fits the interim 200.
        assert!(seg_pursuit_schedule_v1(&interim_clock(200), 10, 20, 1).fits(200));
        // A single-round (inline) class fits trivially: everything before Final.
        let one = seg_pursuit_schedule_v1(&interim_clock(200), 1, 20, 1);
        assert_eq!(one.liability_needed_daa, 0);
    }

    /// Every extra chunk of the demanded material is one more DAA in every round: the need grows linearly, and the ceiling cuts.
    #[test]
    fn the_need_grows_with_the_chunks_and_the_ceiling_refuses_one_over() {
        let clock = interim_clock(200);
        let need = |chunks: u64| seg_pursuit_schedule_v1(&clock, 11, response_window_daa_v1(20, chunks), 1).liability_needed_daa;
        let mut prev = need(1);
        for c in 2..40 {
            let n = need(c);
            assert!(n > prev, "monotone in chunks: {c}");
            prev = n;
        }
        // The largest chunk count that fits each ceiling across W's range, and one more does not.
        for ceiling in [280u64, 300, 358, 400, 436] {
            let max = (1..10_000u64).take_while(|c| need(*c) <= ceiling).last().unwrap();
            assert!(need(max) <= ceiling && need(max + 1) > ceiling, "ceiling {ceiling}: max {max}");
        }
    }

    #[test]
    fn the_filing_chunks_land_after_the_last_service() {
        let s1 = seg_pursuit_schedule_v1(&interim_clock(400), 11, 20, 1);
        let s16 = seg_pursuit_schedule_v1(&interim_clock(400), 11, 20, 16);
        assert_eq!(s16.filed_daa, s1.filed_daa + 15);
        // The deepest round's admission needs 223; the 16-chunk filing lands at 315, 237 after Final (78).
        assert_eq!((s1.liability_needed_daa, s16.liability_needed_daa), (223, 237));
        assert_eq!(s16.liability_needed_daa, s16.filed_daa - s16.final_daa);
    }

    /// An inline (v1–v3) class rides one demand round: whatever its response's chunk count (at most one object), the pursuit ends
    /// before Final. What it does NOT give is reaction time: a maximal one-object response under the base deadline (which inline
    /// demands keep) leaves the producer `court − 15` = 5 DAA to react — less than the interim OPV disclose budget (8). Open item.
    #[test]
    fn one_round_always_fits_and_a_maximal_inline_object_squeezes_the_base_deadline() {
        for chunks in 1..=ROUTE_CHUNKS_PER_OBJECT_V1 {
            let s = seg_pursuit_schedule_v1(&interim_clock(200), 1, response_window_daa_v1(20, chunks), 1);
            assert_eq!(s.liability_needed_daa, 0, "{chunks}");
        }
        assert_eq!(inline_response_chunks_v1(u128::MAX / 4), ROUTE_CHUNKS_PER_OBJECT_V1, "capped at one object");
        let reaction = 20 - (ROUTE_CHUNKS_PER_OBJECT_V1 - 1) * ROUTE_DAA_PER_CHUNK_V1;
        assert_eq!(reaction, 5);
        assert!(reaction < 8, "below the interim OPV disclose budget");
    }

    #[test]
    fn sessions_fit_their_hard_deadline_exactly() {
        let c = SessionClockV1 {
            open_span_daa: 4_200,
            close_span_daa: 5_400,
            session_window_daa: 1_200,
            first_step_daa: 10,
            turnaround_daa: 2,
            prove_daa: 9,
        };
        let s = session_pursuit_schedule_v1(&c, 34);
        assert!(!s.fits);
        assert_eq!(s.max_sessions, 4, "opened at 10, 1212, 2414, 3616 (≤ 4200); a fifth would open at 4818");
        assert!(session_pursuit_schedule_v1(&c, 4).fits && !session_pursuit_schedule_v1(&c, 5).fits);
        // At a 42-DAA turn window 34 sessions fit.
        let turn = SessionClockV1 { session_window_daa: 42, ..c };
        assert!(session_pursuit_schedule_v1(&turn, 34).fits);
    }

    #[test]
    fn population_is_what_one_chunk_a_daa_carries() {
        assert_eq!(concurrent_pursuits_bound_v1(280, 11 * 22 + 1), 1);
        assert_eq!(concurrent_pursuits_bound_v1(280, 12), 23);
        assert_eq!(concurrent_pursuits_bound_v1(280, 0), 280);
    }
}
