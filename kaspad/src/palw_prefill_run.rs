//! **The prefill run width, chosen from the memory a replay can be granted** (int-10.2 A2;
//! `docs/design/palw/t12-replay-memory-1001.md`).
//!
//! # What was measured
//!
//! An 8k replay's need on 5.104 was 3.37 GiB, and 1.67 GiB of it was trace scratch: the dense engine
//! walks a prompt `A16_PREFILL_RUN_POSITIONS` (64) positions at a time through each layer and holds the
//! run's committed traces until the capture takes them — `64 × one position's trace at the widest
//! history (≈ 9.3 MB at 1,024 rows) × 3 live copies` (`palw_resource_profile_v1`,
//! `PALW_PREFILL_RUN_COPIES_V1`). The width is the engine's speed — stepped one position at a time the
//! 1.5B row runs 36.3 ms a position, in runs of 32 16.3 ms, of 64 14.7 ms (the constant's own doc) — and
//! it is a NODE-LOCAL choice: every width commits the same rows, the same cache and the same roots
//! (`the_one_pass_prefill_is_the_position_by_position_one`, ADR-0117 Decision 2). A seat holding a
//! 3.5 GiB share does not need the fastest width; it needs to start.
//!
//! # The rule
//!
//! When a duty reserves memory for a replay (or an attempt), the node picks the WIDEST width in
//! `{cap, 32, 16, 8, 4, 2, 1}` — `cap` is `--palw-prefill-run-max` (default 64, the engine's) — whose
//! derived need the ledger grants now ([`reserve_at_widest_run_v1`]). Narrower widths are tried only
//! while they lower the need: a family whose memory does not move with the width (the floor, the hybrid,
//! an IR class) is priced once and reserved once, as before.
//!
//! # One value, from the reservation to the run
//!
//! The width is SET ON THE BACKEND INSTANCE whose need is derived (`set_prefill_run_positions_v1`), the
//! need is derived FROM that instance (its resource profile reads its own width), and that instance —
//! the same object, moved — is the one the duty then executes. So the figure reserved and the run that
//! spends it cannot disagree. That holds because every duty that reserves resolves an instance of its
//! own (`PalwPanelService::resolve_backend`, the producer's, the audit's, the filer's: a fresh
//! `Box<dyn PalwExecutionBackendV1>` each, moved into its blocking task) — and the one instance that is
//! shared, the executor's kept backend (`executor_backend_v1`, an `Arc`), is never narrowed: this
//! function takes `&mut`, which an `Arc` cannot give, so a shared instance runs at the width it was
//! resolved at (the cap, which the SDK sets on every backend it resolves) and is priced at it.
//!
//! # The producer
//!
//! The same rule: 64 whenever its need fits, narrower only when the attempt would otherwise HOLD. An
//! attempt that runs at 16 positions takes ~1.2× the time (16.3 → ~18 ms a position, interpolated) and an
//! attempt that holds produces nothing; the race against the other producers is lost either way by a
//! held attempt, never by a narrower one.
//!
//! **Node-local, never consensus**: no root, price or verdict reads the width.

use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
use kaspa_core::{info, warn};
use kaspa_hashes::Hash64;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use crate::palw_backends::PalwRoleMemoryNeedV1;

/// **The width a node runs at when nothing narrower is needed: the engine's own**
/// (`misaka_palw_base0::qwen25_a16_backend::A16_PREFILL_RUN_POSITIONS`, the fastest measured).
pub const PALW_PREFILL_RUN_DEFAULT_V1: u32 = misaka_palw_base0::qwen25_a16_backend::A16_PREFILL_RUN_POSITIONS as u32;

/// **The narrowest width the readiness proof may lean on without saying so** (the coordinator's 7th
/// condition, 2026-10-01): a seat that can meet a class only below it replays that class at a quarter of
/// the cap's speed or worse, and the slow tail of the class's receipts is made of seats like it — so the
/// proof still goes out, and the node logs it and counts it in `getPalwNodeStatus.verification`.
pub const PALW_PREFILL_RUN_SLOW_BELOW_V1: u32 = 16;

static CAP: OnceLock<u32> = OnceLock::new();

/// **Arm `--palw-prefill-run-max`**, once, from the daemon; `None` is the default width. Arming twice
/// keeps the first.
pub fn arm_prefill_run_cap_v1(cap: Option<u32>) -> u32 {
    *CAP.get_or_init(|| cap.unwrap_or(PALW_PREFILL_RUN_DEFAULT_V1).clamp(1, PALW_PREFILL_RUN_DEFAULT_V1))
}

/// The armed cap, or the default where no daemon armed one (a tool, a test).
pub fn armed_prefill_run_cap_v1() -> u32 {
    CAP.get().copied().unwrap_or(PALW_PREFILL_RUN_DEFAULT_V1)
}

/// **The widths a reservation tries, widest first**: the cap, then every power of two below it, down to
/// one position at a time.
pub fn palw_prefill_run_candidates_v1(cap: u32) -> Vec<u32> {
    let cap = cap.max(1);
    let mut out = vec![cap];
    let mut w = (cap - 1).checked_next_power_of_two().map_or(cap, |p| if p >= cap { p / 2 } else { p });
    while w >= 1 && w < cap {
        if out.last() != Some(&w) {
            out.push(w);
        }
        if w == 1 {
            break;
        }
        w /= 2;
    }
    out
}

/// A reservation taken at a width: the guard, the need it reserved, and the width the instance now runs
/// at (`None` for a family whose memory does not move with it).
pub struct PalwRunReservationV1<R> {
    pub reserved: R,
    pub need: PalwRoleMemoryNeedV1,
    pub width: Option<u32>,
    /// The need at the cap, when the reservation had to narrow — what a log line compares against.
    pub need_at_cap: Option<PalwRoleMemoryNeedV1>,
}

/// **Reserve at the widest width whose need the ledger grants, and leave the instance at that width.**
///
/// `need_at` derives the role's need from the instance as it stands (the caller's own composition — a
/// full seat, a partial seat's streamed fold, a whole capture); `reserve` asks the ledger. The instance's
/// width is set before each derivation, so the need that is granted is the need of the width the
/// instance is left at, and the instance is what the caller runs. `Err` is the narrowest width's
/// refusal (the least this duty could start with) and its need; the instance is left at the cap.
pub fn reserve_at_widest_run_v1<R>(
    backend: &mut dyn PalwExecutionBackendV1,
    cap: u32,
    mut need_at: impl FnMut(&dyn PalwExecutionBackendV1) -> PalwRoleMemoryNeedV1,
    mut reserve: impl FnMut(&PalwRoleMemoryNeedV1) -> Result<R, String>,
) -> Result<PalwRunReservationV1<R>, (String, PalwRoleMemoryNeedV1)> {
    if backend.prefill_run_positions_v1().is_none() {
        let need = need_at(backend);
        return match reserve(&need) {
            Ok(reserved) => Ok(PalwRunReservationV1 { reserved, need, width: None, need_at_cap: None }),
            Err(why) => Err((why, need)),
        };
    }
    let mut at_cap: Option<PalwRoleMemoryNeedV1> = None;
    let mut refused: Option<(String, PalwRoleMemoryNeedV1)> = None;
    for width in palw_prefill_run_candidates_v1(cap) {
        backend.set_prefill_run_positions_v1(width);
        let need = need_at(backend);
        // A width that does not lower the need cannot be granted where a wider one was refused: stop.
        if refused.as_ref().is_some_and(|(_, wider)| need.total_bytes() >= wider.total_bytes()) {
            break;
        }
        match reserve(&need) {
            Ok(reserved) => {
                note_reserved_v1(width, at_cap.is_some());
                return Ok(PalwRunReservationV1 { reserved, need, width: Some(width), need_at_cap: at_cap });
            }
            Err(why) => {
                if at_cap.is_none() {
                    at_cap = Some(need.clone());
                }
                refused = Some((why, need));
            }
        }
    }
    backend.set_prefill_run_positions_v1(cap);
    Err(refused.expect("at least the cap was tried"))
}

// -------------------------------------------------------------------------------------------------
// What the node reports
// -------------------------------------------------------------------------------------------------

static LAST_WIDTH: AtomicU32 = AtomicU32::new(0);
static NARROWED: AtomicU64 = AtomicU64::new(0);

fn note_reserved_v1(width: u32, narrowed: bool) {
    LAST_WIDTH.store(width, Ordering::Relaxed);
    if narrowed {
        NARROWED.fetch_add(1, Ordering::Relaxed);
    }
}

/// The classes this seat can meet only below [`PALW_PREFILL_RUN_SLOW_BELOW_V1`], with the widest width it
/// can (0: none at all).
fn capacity_widths() -> &'static Mutex<HashMap<Hash64, u32>> {
    static WIDTHS: OnceLock<Mutex<HashMap<Hash64, u32>>> = OnceLock::new();
    WIDTHS.get_or_init(Default::default)
}

/// **Record what the readiness proof found for a class**: the widest width at which this host's capacity
/// admits a full-seat replay of it. Below [`PALW_PREFILL_RUN_SLOW_BELOW_V1`] it is logged (once a class
/// changes its answer) and counted, so the slow tail of a class's receipts is visible before it is felt.
pub fn note_capacity_width_v1(role: &str, class_id: Hash64, width: Option<u32>, need: &PalwRoleMemoryNeedV1) {
    let Some(width) = width else { return };
    let mut widths = capacity_widths().lock().unwrap_or_else(|p| p.into_inner());
    if width >= PALW_PREFILL_RUN_SLOW_BELOW_V1.min(armed_prefill_run_cap_v1()) {
        widths.remove(&class_id);
        return;
    }
    if widths.insert(class_id, width) != Some(width) {
        warn!(
            "[{role}] class {class_id}: this host can replay it only at a prefill run of {width} position(s) (the cap is {}): {} — \
             its proof still goes out, and its replays here run at {width}, slower than a seat at {} (int-10.2 A2)",
            armed_prefill_run_cap_v1(),
            need.describe(),
            PALW_PREFILL_RUN_SLOW_BELOW_V1
        );
    }
}

/// **The width half of `getPalwNodeStatus.verification`**: `run_cap` (the cap), `run_last` (the width the
/// last reservation took, 0 before the first), `run_narrowed` (reservations that had to narrow since the
/// start), and `capacity_run_min` / `capacity_narrow_classes` — the narrowest width at which this host can
/// meet one of its classes, and how many it meets only below 16 (0 / 0 when none).
pub fn prefill_run_status_v1() -> String {
    let widths = capacity_widths().lock().unwrap_or_else(|p| p.into_inner());
    format!(
        "run_cap={} run_last={} run_narrowed={} capacity_run_min={} capacity_narrow_classes={}",
        armed_prefill_run_cap_v1(),
        LAST_WIDTH.load(Ordering::Relaxed),
        NARROWED.load(Ordering::Relaxed),
        widths.values().min().copied().unwrap_or(0),
        widths.len()
    )
}

/// **A reservation the ledger made narrower than the cap, said** — at most once a minute per
/// `(log_role, duty_role)`, since a seat narrowing every replay of a busy hour is one fact: the duty, the
/// need it reserved at the width it runs at, and what the cap's width would have needed. Counted in
/// `run_narrowed` whether or not the line prints.
pub fn note_narrowed_v1<R>(log_role: &str, duty_role: &str, what: &str, taken: &PalwRunReservationV1<R>) {
    static LAST: OnceLock<Mutex<HashMap<String, std::time::Instant>>> = OnceLock::new();
    let (Some(width), Some(at_cap)) = (taken.width, &taken.need_at_cap) else { return };
    let key = format!("{log_role}/{duty_role}");
    {
        let mut last = LAST.get_or_init(Default::default).lock().unwrap_or_else(|p| p.into_inner());
        let now = std::time::Instant::now();
        if last.get(&key).is_some_and(|at| now.duration_since(*at) < std::time::Duration::from_secs(60)) {
            return;
        }
        last.insert(key, now);
    }
    info!(
        "[{log_role}] {what}: the {duty_role} reservation runs at a prefill run of {width} position(s) — {} — because at the cap's \
         {} it needed {:.2} GiB, which the memory ledger could not grant (int-10.2 A2; every width commits the same roots)",
        taken.need.describe(),
        armed_prefill_run_cap_v1(),
        at_cap.total_bytes() as f64 / (1u64 << 30) as f64
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_memory_ledger::{PalwMemoryLedgerV1, PalwMemoryPoolV1, PalwMemoryReservationKeyV1};
    use kaspa_consensus_core::palw_backend::{PalwClaimRootsV1, PalwExecutionOutcomeV1, PalwMaterialVerdictV1};
    use kaspa_consensus_core::palw_resource_profile_v1::{
        PalwCaptureRetentionV1, PalwResourceRoleV1, PalwRuntimeLimitsV1, PalwRuntimeProfileV1, palw_resource_profile_v1,
    };
    use kaspa_consensus_core::palw_v2::PalwJobContextV2;

    const GIB: f64 = (1u64 << 30) as f64;
    const MIB: u64 = 1 << 20;
    /// The 8k `.palwart`: 1,799,359,436 bytes (the committed sidecar the genesis card reads).
    const ART_8K: u64 = 1_799_359_436;

    #[test]
    fn the_widths_tried_are_the_cap_then_every_power_of_two_below_it() {
        assert_eq!(palw_prefill_run_candidates_v1(64), vec![64, 32, 16, 8, 4, 2, 1]);
        assert_eq!(palw_prefill_run_candidates_v1(32), vec![32, 16, 8, 4, 2, 1]);
        assert_eq!(palw_prefill_run_candidates_v1(48), vec![48, 32, 16, 8, 4, 2, 1], "a cap that is no power of two is tried first");
        assert_eq!(palw_prefill_run_candidates_v1(1), vec![1]);
        assert_eq!(palw_prefill_run_candidates_v1(0), vec![1], "zero reads as one");
        assert_eq!(palw_prefill_run_candidates_v1(3), vec![3, 2, 1]);
    }

    /// testnet-12's 8k row (`Qwen/Qwen2.5-1.5B/graph-v7@8192`, canonical (1023, 2)) as a seat's full
    /// replay derives it, at `width`, under A16-KV-i16 on an 8-thread host, the file reserved or pinned.
    fn eight_k_full_seat(width: u32, pinned: bool) -> PalwRoleMemoryNeedV1 {
        use misaka_palw_base0::classes::{A16_GRAPH_V7_8K_MODEL_ID, a16_graph_v7_row_v1};
        let row = a16_graph_v7_row_v1(8_192, A16_GRAPH_V7_8K_MODEL_ID).expect("the 8k row");
        let profile = &row.profile;
        let job = kaspa_consensus_core::palw_base0_profile::rc_job_context(profile, row.canonical_job.0, row.canonical_job.1);
        let ladder = kaspa_consensus_core::palw_state_chunk_map::palw_class_step_ladder_v1(
            kaspa_consensus_core::palw_state_chunk_map::PALW_HELD_STEP_LADDER_V1,
            profile,
        );
        let leaves = kaspa_consensus_core::palw_step::step_leaf_count_capped_v1(profile, &job, ladder).expect("its step space");
        let fold = PalwCaptureRetentionV1::Fold {
            retain_level: misaka_palw_base0::fp_capture::palw_base0_sparse_retain_level_for_class_v1(profile, ladder),
        };
        let limits = PalwRuntimeLimitsV1 { threads: 8, prefill_run_positions: width };
        let p = palw_resource_profile_v1(
            profile,
            &job,
            leaves,
            PalwRuntimeProfileV1::A16KvI16,
            PalwResourceRoleV1::FullSeat,
            limits,
            fold,
        )
        .expect("derives");
        PalwRoleMemoryNeedV1 {
            role: PalwResourceRoleV1::FullSeat,
            holding_bytes: if pinned { 0 } else { ART_8K },
            pinned_bytes: if pinned { ART_8K } else { 0 },
            prefill_run_positions: Some(width),
            derived_bytes: 0,
            runtime: Some(PalwRuntimeProfileV1::A16KvI16),
            profile: Some(p),
        }
    }

    /// **The 8k replay's need, pinned to the figures int-10.2 is built on.** The 10-01 line on 5.104 —
    /// `3.37 GiB as full-seat (artifact 1.68 GiB + K/V 0.03 GiB at 1024 rows + … trace scratch 1.67 GiB …)`
    /// — is the unpinned figure at 64; pinned (A1) the artifact leaves the need and 1.70 GiB remain; the
    /// trace scratch halves with each halving of the width (A2): ~0.87 GiB at 32, ~0.45 at 16. Nothing
    /// but the trace scratch moves with the width.
    #[test]
    fn the_8k_full_seat_need_falls_from_3_37_gib_with_the_pin_and_the_width() {
        let gib = |need: &PalwRoleMemoryNeedV1| need.total_bytes() as f64 / GIB;
        let today = eight_k_full_seat(64, false);
        assert!((3.35..3.40).contains(&gib(&today)), "the 10-01 line: {}", today.describe());
        let p = today.profile.expect("a profile");
        assert_eq!(p.end_rows, 1_024, "K/V at 1024 rows");
        assert!((1.65..1.70).contains(&(p.trace_scratch_bytes as f64 / GIB)), "trace scratch 1.67 GiB: {}", p.trace_scratch_bytes);
        let pinned = eight_k_full_seat(64, true);
        assert!((1.67..1.73).contains(&gib(&pinned)), "pinned at 64: {}", pinned.describe());
        assert_eq!(today.total_bytes() - pinned.total_bytes(), ART_8K, "the pin takes exactly the file out of the need");
        assert!(pinned.describe().contains("1.68 GiB pinned: resident once on this host, not reserved"), "{}", pinned.describe());
        let w32 = eight_k_full_seat(32, true);
        let w16 = eight_k_full_seat(16, true);
        assert!((0.84..0.90).contains(&gib(&w32)), "W=32: {}", w32.describe());
        assert!((0.42..0.48).contains(&gib(&w16)), "W=16: {}", w16.describe());
        assert!(w16.describe().contains("at a prefill run of 16"), "{}", w16.describe());
        for width in [32u32, 16, 8, 4, 2, 1] {
            let (wide, narrow) = (eight_k_full_seat(64, true).profile.unwrap(), eight_k_full_seat(width, true).profile.unwrap());
            assert_eq!(
                narrow.trace_scratch_bytes * 64,
                wide.trace_scratch_bytes * u64::from(width),
                "{width}: the trace scratch is the width's"
            );
            assert_eq!(
                narrow.working_set_bytes() - narrow.trace_scratch_bytes,
                wide.working_set_bytes() - wide.trace_scratch_bytes,
                "{width}: nothing else moves"
            );
        }
        let one = eight_k_full_seat(1, true);
        assert!(gib(&one) < 0.06, "one position at a time the 8k replay is its K/V and a trace: {}", one.describe());
        for width in palw_prefill_run_candidates_v1(64) {
            eprintln!(
                "8k full seat at a prefill run of {width:>2}: unpinned {:.3} GiB, pinned {:.3} GiB",
                gib(&eight_k_full_seat(width, false)),
                gib(&eight_k_full_seat(width, true))
            );
        }
    }

    /// A backend whose need is a function of its width, and nothing else — the reservation's contract
    /// without an engine. `None` from `prefill_run_positions_v1` when `moves` is false: a family whose
    /// memory does not move with the width.
    struct Widths {
        width: u32,
        moves: bool,
    }

    impl PalwExecutionBackendV1 for Widths {
        fn model_id(&self) -> &str {
            "widths"
        }
        fn job_for_anchor(&self, _anchor: Hash64) -> Result<(PalwJobContextV2, Vec<usize>), String> {
            Err("not used".into())
        }
        fn execute(&self, _job: &PalwJobContextV2, _prompt: &[usize]) -> Result<PalwExecutionOutcomeV1, String> {
            Err("not used".into())
        }
        fn verify_material(&self, _material: &[u8], _claim: PalwClaimRootsV1) -> PalwMaterialVerdictV1 {
            PalwMaterialVerdictV1::Unverifiable
        }
        fn prefill_run_positions_v1(&self) -> Option<u32> {
            self.moves.then_some(self.width)
        }
        fn set_prefill_run_positions_v1(&mut self, positions: u32) {
            self.width = positions.max(1);
        }
    }

    /// The 8k need the stub's width reads (pinned), or the width-blind floor's 0.5 GiB.
    fn need_of(b: &dyn PalwExecutionBackendV1) -> PalwRoleMemoryNeedV1 {
        match b.prefill_run_positions_v1() {
            Some(width) => eight_k_full_seat(width, true),
            None => PalwRoleMemoryNeedV1 {
                role: PalwResourceRoleV1::FullSeat,
                holding_bytes: 0,
                pinned_bytes: 0,
                prefill_run_positions: None,
                derived_bytes: 0,
                runtime: None,
                profile: None,
            },
        }
    }

    fn key(job: u64) -> PalwMemoryReservationKeyV1 {
        PalwMemoryReservationKeyV1 { role: "full-seat", class_id: Hash64::from_u64_word(0xEBF4), job: Hash64::from_u64_word(job) }
    }

    /// **A 5.104 seat, replayed: the widest width the share grants, and the instance left at it.** A
    /// 3,584 MiB share with the readiness proof's 32 MiB carve. The first 8k replay is granted at the
    /// cap (1.70 GiB, pinned); a second beside it is too (the two fit — what one 3.37 GiB replay alone
    /// used to fill); a third is narrowed to the widest width the rest holds; when not even one position
    /// at a time fits, the refusal is the narrowest width's and the instance goes back to the cap.
    /// Every grant's need is the need OF the width the instance is left at — the one value a run spends.
    #[test]
    fn a_reservation_takes_the_widest_width_the_ledger_grants_and_leaves_the_instance_there() {
        let ledger = PalwMemoryLedgerV1::new_with_proof_carve(
            PalwMemoryPoolV1::Host,
            Some(3_584 * MIB),
            crate::palw_memory_ledger::PALW_READINESS_PROOF_CARVE_BYTES_V1,
            || None,
        );
        let reserve = |job: u64| {
            let ledger = std::sync::Arc::clone(&ledger);
            move |need: &PalwRoleMemoryNeedV1| ledger.reserve(key(job), need.total_bytes()).map_err(|e| e.to_string())
        };
        let mut first = Widths { width: 64, moves: true };
        let a = reserve_at_widest_run_v1(&mut first, 64, need_of, reserve(1)).expect("the first fits at the cap");
        assert_eq!((a.width, first.width, a.need_at_cap.is_none()), (Some(64), 64, true));
        let mut second = Widths { width: 64, moves: true };
        let b = reserve_at_widest_run_v1(&mut second, 64, need_of, reserve(2)).expect("a second 8k replay beside it");
        assert_eq!(b.width, Some(64), "two pinned 8k replays at 64 fit the share one unpinned replay filled");
        let mut third = Widths { width: 64, moves: true };
        let c = reserve_at_widest_run_v1(&mut third, 64, need_of, reserve(3)).expect("narrowed, not held");
        let width = c.width.expect("a width");
        assert!(width < 64 && third.width == width, "the instance runs at the width reserved: {width}");
        assert_eq!(c.need, eight_k_full_seat(width, true), "and the need reserved is that width's");
        assert_eq!(c.need_at_cap.as_ref().map(|n| n.total_bytes()), Some(eight_k_full_seat(64, true).total_bytes()));
        let left =
            3_584 * MIB - crate::palw_memory_ledger::PALW_READINESS_PROOF_CARVE_BYTES_V1 - a.need.total_bytes() - b.need.total_bytes();
        assert!(c.need.total_bytes() <= left, "it fits what was left");
        assert!(eight_k_full_seat(width * 2, true).total_bytes() > left, "and twice the width would not have");
        assert!(prefill_run_status_v1().contains("run_cap="));
        // Nothing fits: the narrowest width's refusal, and the instance back at the cap.
        let mut fourth = Widths { width: 64, moves: true };
        let filler = ledger.reserve(key(9), ledger.snapshot().available_bytes.expect("bounded") - 1).expect("the rest");
        let (why, need) = match reserve_at_widest_run_v1(&mut fourth, 64, need_of, reserve(4)) {
            Err(refused) => refused,
            Ok(_) => panic!("nothing is left"),
        };
        assert_eq!(need, eight_k_full_seat(1, true), "the refusal names the least this duty could start with");
        assert!(why.contains("cannot cover"), "{why}");
        assert_eq!(fourth.width, 64, "an instance that will not run is left at the cap");
        drop((a, b, c, filler));
        // A family whose memory does not move with the width is priced once and reserved once.
        let mut floor = Widths { width: 64, moves: false };
        let mut asked = 0;
        let taken = reserve_at_widest_run_v1(
            &mut floor,
            64,
            |b| {
                asked += 1;
                need_of(b)
            },
            reserve(5),
        )
        .expect("half a GiB fits");
        assert_eq!((taken.width, asked), (None, 1), "one derivation, no width");
    }

    /// **The width reserved is the width that runs, on the real dense engine.** A small graph-v5 A16
    /// class: a reservation the ledger can grant only at 16 leaves the instance at 16, the need it
    /// reserved is the instance's own figure, and the instance — run — commits the cap's roots.
    #[test]
    fn the_instance_reserved_at_a_width_runs_at_it_and_commits_the_caps_roots() {
        use kaspa_consensus_core::palw_qwen25_profile::{PalwQwen25GeometryV1, qwen25_a16_profile_v5};
        use misaka_palw_base0::artifact::{Base0ArtifactV1, Base0ShapeV1, LN_THETA_10000_GEN_Q};
        let geometry = PalwQwen25GeometryV1 {
            layer_count: 2,
            hidden_dim: 8,
            ffn_dim: 8,
            attn_heads: 2,
            attn_kv_heads: 2,
            attn_head_dim: 4,
            vocab_size: 64,
            n_ctx: 128,
            n_threads: 1,
            rms_eps_q: 1,
            tile_len: 4,
        };
        let shape = Base0ShapeV1 {
            n_layers: 2,
            n_heads: 2,
            n_kv_heads: 2,
            d_head: 4,
            d_ff: 8,
            vocab: 64,
            max_position: 128,
            ln_theta_gen_q: LN_THETA_10000_GEN_Q,
            eps_q: 1,
        };
        let artifact = std::sync::Arc::new(
            Base0ArtifactV1::derive_deterministic(shape, 0x1001)
                .expect("a valid shape")
                .with_a16_params(misaka_palw_base0::engine_a16::derived_a16_store(&shape))
                .expect("sorted and unique"),
        );
        let profile = qwen25_a16_profile_v5(geometry).expect("the v5 row projects");
        let build = || {
            misaka_palw_base0::qwen25_a16_backend::Qwen25A16Backend::new(
                artifact.clone(),
                b"misaka-palw-rc".to_vec(),
                profile.clone(),
                (60, 3),
            )
            .expect("the fixture's declaration is the engine's program")
        };
        let reference = build();
        let (job, prompt) = reference.job_for_anchor(Hash64::from_u64_word(0x1001)).expect("a job");
        let need_at = |b: &dyn PalwExecutionBackendV1| PalwRoleMemoryNeedV1 {
            role: PalwResourceRoleV1::FullSeat,
            holding_bytes: 0,
            pinned_bytes: 0,
            prefill_run_positions: b.prefill_run_positions_v1(),
            derived_bytes: 0,
            runtime: b.runtime_profile_v1(),
            profile: b.resource_profile_v1(Some(&job), PalwResourceRoleV1::FullSeat),
        };
        // A share that holds the width-16 figure and not the width-32 one.
        let mut probe = build();
        probe.set_prefill_run_positions_v1(16);
        let at_16 = need_at(&probe).total_bytes();
        probe.set_prefill_run_positions_v1(32);
        let at_32 = need_at(&probe).total_bytes();
        assert!(at_16 < at_32, "the fixture's need moves with the width");
        let ledger = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, Some(at_16 + (at_32 - at_16) / 2), || None);
        let mut instance: Box<dyn PalwExecutionBackendV1> = Box::new(build());
        let taken = reserve_at_widest_run_v1(instance.as_mut(), 64, need_at, |need| {
            ledger.reserve(key(1), need.total_bytes()).map_err(|e| e.to_string())
        })
        .expect("granted at 16");
        assert_eq!(
            (taken.width, instance.prefill_run_positions_v1(), taken.need.prefill_run_positions),
            (Some(16), Some(16), Some(16))
        );
        assert_eq!(taken.need, need_at(instance.as_ref()), "the need reserved is the instance's own, at the width it runs");
        let ran = instance.execute_for_verdict(&job, &prompt).expect("the narrowed instance replays");
        let want = reference.execute_for_verdict(&job, &prompt).expect("the cap's");
        assert_eq!((ran.execution_root, ran.trace_root, ran.work_leaves), (want.execution_root, want.trace_root, want.work_leaves));
    }
}
