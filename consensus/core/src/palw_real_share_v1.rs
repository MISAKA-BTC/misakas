//! **The Useful Work Transition's two consensus rules (ADR-0165): the floor is the idle-only bonded fallback,
//! and the work carries the clock.**
//!
//! * **`Params::palw_floor_reserve_v1` (A″).** Past it a `PALW-BASE-0` attempt is accepted only while the chain's
//!   *floor state* is **Idle** and is refused by name (`FloorNotIdle`) otherwise — a pre-write refusal the fold skips
//!   like every other, so a floor attempt refused in Probe or Normal earns **no claim, no reward and no PALW
//!   weight** (it is still a block in the DAG and in GHOSTDAG's colouring: ADR-0165 §00.2). The floor state is one
//!   small rooted value ([`PalwFloorStateV1`]), moved by ONE pure function ([`palw_floor_step_v1`]) from the
//!   REAL attempts this branch's fold FULLY ACCEPTED, with the colour each had in the accepting block's mergeset:
//!
//!   ```text
//!   Idle ──(a BLUE REAL)──▶ Normal{last_blue = daa}
//!   Idle ──(a RED REAL, if the cooldown has run)──▶ Probe{until = daa + probe_slots}
//!   Probe ──(a BLUE REAL)──▶ Normal{last_blue = daa}          Probe ──(daa ≥ until)──▶ Idle (last_probe_end = until)
//!   Normal ──(a BLUE REAL)──▶ Normal{last_blue = daa}         Normal ──(daa − last_blue > floor_idle_slots)──▶ Idle
//!   a RED REAL never extends Probe or Normal.
//!   ```
//!
//!   Floors are valid only in Idle. A REAL attempt is an attempt of any class but the base class — and it moves the
//!   machine only when it is **fully accepted**: it passed every check the fold makes (the registered, Active class, the
//!   bond, the budget, the lottery, the room, the share…) and **wrote its claim**. The kind a header claims, and an attempt
//!   the fold skipped, never move it. A BLUE one is verified success — it needs no cooldown; the cooldown exists only to
//!   stop a stream of RED ones from suppressing the floor. **And only an attempt-lane BLOCK's attempt is an event** (the
//!   block's own, or a merged one, with its mergeset colour): **a capacity rider** (ADR-0164 F-M1, tag 95) rides an object,
//!   has no header and is in no colouring, so it is no event at all — neither BLUE nor RED. The machine exists to shield REAL
//!   attempt blocks from the floor blocks that colour against them; a rider is no block, so counting it would refuse the
//!   floor without shielding anything and would open a keep-alive that costs no block.
//! * **`Params::palw_real_clock_tick_v1` (B).** An attempt-lane block is a tick source beside the heartbeat. The
//!   DAA score still advances **once per clock slot** however many attempts a slot holds.
//!
//! This module holds the pure parts: the floor state machine, the tick-source rule and the fences' plumbing.

use crate::config::params::{ForkActivation, PalwPostLaunchFenceV1, Params};
use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};
use borsh::{BorshDeserialize, BorshSerialize};

/// One clock slot, in milliseconds — ADR-0142's recovery interval, the cadence the DAA moves at.
pub const PALW_REAL_SLOT_MS_V1: u64 = crate::palw_heartbeat_v1::HEARTBEAT_RECOVERY_INTERVAL_MS;

/// **The measurements the constants below are chosen from** (P2's, `lanes/evidence/8k-red-1003/`: the last 300 DAA of
/// testnet-12 before 2026-10-03, 72 attempts of the 8k class): the 95th percentile of the gap between two accepted REAL
/// attempts, **17.3 slots** (p50 3.1, max 30.6; the long gaps are panel and backpressure holds). The inference is INSIDE
/// the gap — the gap runs from one acceptance to the next, and the next attempt is drawn within it.
pub const PALW_REAL_MEASURED_GAP_P95_MS_V1: u64 = 2_076_000;

/// **The 8k producer's draws, from P2's second replay** (`lanes/evidence/8k-red-1003/replay2/`, the same capture): one draw —
/// a template, the inference, the submission — takes **325 s at the median and 426 s at the longest** (3 and 4 slots, rounded
/// up). The probe is chosen from these, not from the first replay's inference figures (342 s at p50, 418 s at p95, 4 slots).
pub const PALW_REAL_MEASURED_DRAW_P50_MS_V1: u64 = 325_000;
pub const PALW_REAL_MEASURED_DRAW_MAX_MS_V1: u64 = 426_000;

/// **The margin over the measured gap, in slots**: one for the accepting block's lag (a REAL attempt is accepted at the
/// block that merges it, a slot or so after it was drawn) and one for the rounding and the clock's jitter.
pub const PALW_FLOOR_IDLE_MARGIN_SLOTS_V1: u64 = 2;

/// **`floor_idle_slots`: how many slots Normal outlives its last BLUE REAL attempt** — **20 slots, 40 minutes** (the
/// coordinator's decision of 2026-10-03, kept by P2's second replay: with full compliance and a probe of 8, REAL attempts are
/// 93 % of the BLUE attempts GHOSTDAG counts on the live capture at 20, against 47 % at 12 and 69 % at 16). While REAL work lands
/// BLUE at least that often the floor stays refused, so a slow REAL attempt in flight is not buried by floor attempts that
/// colour classically against it (testnet-12's `ghostdag_k` = 1: two floors in its anticone make it RED). **The price:**
/// after REAL work stops the chain is heartbeat-only (unbonded, weight ε a block) for up to this many slots before the
/// bonded floor resumes (ADR-0165 §00.4). Derived by [`palw_floor_idle_slots_for_v1`] and pinned by
/// `the_constants_are_the_measured_ones_and_bounded`; hashed with the fence (`palw_floor_reserve_value_v1`): changing it
/// is a new network id.
pub const PALW_FLOOR_IDLE_SLOTS_V1: u64 = 20;

/// **`probe_slots`: how long a Probe holds the floor refused so the next REAL attempts can land BLUE** — **8 slots, 16
/// minutes** (the coordinator's decision of 2026-10-03 on P2's second replay). A Probe opens when a REAL attempt lands RED
/// (the trigger). The producer's NEXT attempt was templated before the probe began — floors accepted before it are in its
/// anticone, so it collides with the probe's start and may go RED — and the one after it is the first templated under the
/// probe, with no floor under it. The probe must outlast that third attempt's whole draw: the second's median draw, the
/// third's longest and one slot of accepting-block lag ([`palw_floor_probe_slots_for_v1`]: 3 + 4 + 1). Measured with full
/// compliance on the live capture: a probe of 4 or 6 slots gives 63–89 % of REAL attempts BLUE, a probe of 8 or more gives
/// 93–99 % (idle window 12–30).
pub const PALW_FLOOR_PROBE_SLOTS_V1: u64 = 8;

/// **`probe_cooldown_slots`: the least gap from the end of one unanswered Probe to the start of the next** — **20
/// slots**, so a stream of RED REAL attempts keeps the floor refused for at most `probe_slots` of every
/// `probe_slots + probe_cooldown_slots` (8 of 28).
pub const PALW_FLOOR_PROBE_COOLDOWN_SLOTS_V1: u64 = 20;

/// **The ceiling on `floor_idle_slots`, in slots (one hour — the merge-depth duration, 3,600 s).** The weak stretch after
/// REAL work stops is at most `floor_idle_slots`, so its ceiling is the longest such stretch the network accepts.
pub const PALW_FLOOR_IDLE_SLOTS_MAX_V1: u64 = 30;

/// **`floor_idle_slots` from a measurement**: `⌈p95 gap between accepted REAL attempts / slot⌉ + margin`, at least 1 and at
/// most [`PALW_FLOOR_IDLE_SLOTS_MAX_V1`]. Pure and `const`.
pub const fn palw_floor_idle_slots_for_v1(p95_gap_ms: u64, margin_slots: u64) -> u64 {
    let k = p95_gap_ms.div_ceil(PALW_REAL_SLOT_MS_V1).saturating_add(margin_slots);
    if k < 1 {
        1
    } else if k > PALW_FLOOR_IDLE_SLOTS_MAX_V1 {
        PALW_FLOOR_IDLE_SLOTS_MAX_V1
    } else {
        k
    }
}

/// **`probe_slots` from a measurement**: `⌈median draw / slot⌉ + ⌈longest draw / slot⌉ + accepting-block lag` — the attempt
/// templated before the probe (its draw is the median's, and it collides with the probe's start), the first one templated under
/// the probe (its draw is the longest's), and one slot of lag before the block that accepts it.
pub const fn palw_floor_probe_slots_for_v1(p50_draw_ms: u64, max_draw_ms: u64, lag_slots: u64) -> u64 {
    p50_draw_ms.div_ceil(PALW_REAL_SLOT_MS_V1).saturating_add(max_draw_ms.div_ceil(PALW_REAL_SLOT_MS_V1)).saturating_add(lag_slots)
}

/// **Where the chain stands, as far as the floor is concerned.**
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwFloorModeV1 {
    /// No REAL attempt is flowing: the bonded floor is the fallback and is accepted.
    Idle,
    /// A REAL attempt was just accepted after an idle stretch: the floor is refused through slot `until − 1`, so the next
    /// REAL attempt can land BLUE.
    Probe { until: u64 },
    /// REAL work is landing BLUE: the floor is refused while `daa − last_blue ≤ floor_idle_slots`.
    Normal { last_blue: u64 },
}

/// **The rooted floor state** (`PalwChainStateV2::floor_state`; `None` in the state is [`Self::default`]): the mode and
/// the DAA at which the last UNANSWERED probe ended, the cooldown's reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwFloorStateV1 {
    pub mode: PalwFloorModeV1,
    /// The DAA a probe last expired with no BLUE REAL attempt in it (`until`); `None` until one has. Carried through
    /// Probe and Normal unchanged. Only a probe that expires writes it, so Normal → Idle leaves it where it was — the probe
    /// that led to Normal began at least `floor_idle_slots + 1` slots before Normal ended, so its cooldown has run.
    pub last_probe_end: Option<u64>,
}

impl Default for PalwFloorStateV1 {
    fn default() -> Self {
        Self { mode: PalwFloorModeV1::Idle, last_probe_end: None }
    }
}

impl PalwFloorStateV1 {
    /// Is this the state a chain that never saw REAL work holds (`None` in the rooted state)?
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// **Does the floor stand as a valid fallback at `daa`?** The state advanced to `daa`, and Idle.
    pub fn admits_floor_at(&self, daa: u64) -> bool {
        palw_floor_step_v1(*self, daa, None).mode == PalwFloorModeV1::Idle
    }

    /// The canonical rooted form: `None` for the default, so a chain that never left Idle roots as one that never
    /// carried the field.
    pub fn canonical(self) -> Option<Self> {
        (!self.is_default()).then_some(self)
    }
}

impl std::fmt::Display for PalwFloorStateV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.mode {
            PalwFloorModeV1::Idle => match self.last_probe_end {
                Some(end) => write!(f, "idle (the last probe ended unanswered at DAA {end})"),
                None => write!(f, "idle"),
            },
            PalwFloorModeV1::Probe { until } => write!(f, "probe (floors refused until DAA {until})"),
            PalwFloorModeV1::Normal { last_blue } => {
                write!(f, "normal (the last BLUE REAL attempt was accepted at DAA {last_blue})")
            }
        }
    }
}

/// **One REAL attempt BLOCK's attempt FULLY ACCEPTED by the fold** — the claim written — with the colour it had in the accepting
/// block's mergeset. The block's own attempt is BLUE (a chain block); a merged attempt is BLUE or RED as the mergeset says. A
/// capacity rider is not this: it is no event (the fold never builds one for it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwRealAcceptedV1 {
    pub blue: bool,
}

/// **THE transition — one pure function, the floor state machine** (ADR-0165 §00.1).
///
/// `daa` is the accepting block's DAA score (the slot). **Time first**, then the event:
///
/// 1. *time* — Normal becomes Idle once `daa − last_blue > floor_idle_slots`; a Probe becomes Idle once `daa ≥ until`,
///    recording `last_probe_end = until` (the nominal end, never the DAA a block happened to notice it at, so the result does
///    not depend on block spacing). Idempotent and path independent: stepping to `t1` then `t2` is stepping to `t2`.
/// 2. *event* — a FULLY accepted REAL attempt (every fold check passed, its claim written): a BLUE one makes the state
///    Normal with `last_blue = daa` from Idle, from Probe and from Normal alike — BLUE is verified success and needs no
///    cooldown; a RED one changes nothing in Normal or Probe, and in Idle starts a Probe (`until = daa + probe_slots`) if
///    `daa − last_probe_end ≥ probe_cooldown_slots` (or no probe has ended), and otherwise changes nothing.
///
/// **Several attempts in one block apply in the fold's order** — the block's own first, then the merged works in consensus
/// acceptance order — each stepping the state the one before left, at the same `daa`: Idle + [RED, BLUE] is Probe then
/// Normal; Idle + [BLUE, RED] is Normal (the RED then changes nothing). A floor attempt's gate reads the state at its place
/// in that order.
pub fn palw_floor_step_v1(state: PalwFloorStateV1, daa: u64, accepted: Option<PalwRealAcceptedV1>) -> PalwFloorStateV1 {
    use PalwFloorModeV1::{Idle, Normal, Probe};
    let mut next = state;
    match next.mode {
        Normal { last_blue } if daa.saturating_sub(last_blue) > PALW_FLOOR_IDLE_SLOTS_V1 => next.mode = Idle,
        Probe { until } if daa >= until => {
            next.mode = Idle;
            next.last_probe_end = Some(until);
        }
        _ => {}
    }
    if let Some(event) = accepted {
        if event.blue {
            // Verified success, in any mode: Normal, refreshed.
            next.mode = Normal { last_blue: daa };
        } else if next.mode == Idle {
            // A RED one never extends Probe or Normal; in Idle it opens a probe — once per cooldown.
            let cooldown_has_run = next.last_probe_end.is_none_or(|end| daa.saturating_sub(end) >= PALW_FLOOR_PROBE_COOLDOWN_SLOTS_V1);
            if cooldown_has_run {
                next.mode = Probe { until: daa.saturating_add(PALW_FLOOR_PROBE_SLOTS_V1) };
            }
        }
    }
    next
}

impl PalwFloorModeV1 {
    /// The mode's name in the log and in the drill's evidence: `idle`, `probe`, `normal`.
    pub const fn name(&self) -> &'static str {
        match self {
            PalwFloorModeV1::Idle => "idle",
            PalwFloorModeV1::Probe { .. } => "probe",
            PalwFloorModeV1::Normal { .. } => "normal",
        }
    }
}

/// **The words a floor refusal carries.** `PalwStateV2Error::FloorNotIdle`'s message contains them, the producer's hold line
/// carries that message in its `registry="…"` field, and the drill reads them (`PALW_FLOOR_NOT_IDLE_MARKER_V1` is what a hold
/// of the floor producer is grepped by, and what `--palw-drill-floor-ignore-policy` recognises). A test pins the message to it,
/// so rewording the error breaks the build of the test, not the drill.
pub const PALW_FLOOR_NOT_IDLE_MARKER_V1: &str = "idle-only fallback";

/// Does a producer's class-admission refusal say the floor is not idle? (The refusal reaches the producer as text.)
pub fn palw_is_floor_not_idle_refusal_v1(refusal: &str) -> bool {
    refusal.contains(PALW_FLOOR_NOT_IDLE_MARKER_V1)
}

/// **The stable prefix of the node's floor-state line** (`[palw-floor-state]`): one info line per chain block whose fold moved
/// the floor state, naming the move. The drill and an operator grep it.
pub const PALW_FLOOR_STATE_LOG_TAG_V1: &str = "[palw-floor-state]";

/// **The floor-state line**: `[palw-floor-state] daa=<N> block=<H> <from>-><to> last_blue=<n|-> until=<n|-> last_probe_end=<n|->`,
/// where `from` and `to` are `idle`, `probe` or `normal` and the three fields describe the NEW state (`-` where the mode has no
/// such value, or no probe has ended). Pure, so the format is pinned by a test.
pub fn palw_floor_state_log_line_v1(block: &dyn std::fmt::Display, daa: u64, from: &PalwFloorStateV1, to: &PalwFloorStateV1) -> String {
    let field = |v: Option<u64>| v.map_or_else(|| "-".to_string(), |v| v.to_string());
    let (last_blue, until) = match to.mode {
        PalwFloorModeV1::Idle => (None, None),
        PalwFloorModeV1::Probe { until } => (None, Some(until)),
        PalwFloorModeV1::Normal { last_blue } => (Some(last_blue), None),
    };
    format!(
        "{PALW_FLOOR_STATE_LOG_TAG_V1} daa={daa} block={block} {}->{} last_blue={} until={} last_probe_end={}",
        from.mode.name(),
        to.mode.name(),
        field(last_blue),
        field(until),
        field(to.last_probe_end)
    )
}

/// **The net move one block's delta made to the floor state**, `(from, to)`, or `None` if it made none: the first
/// [`PalwDeltaEntryV2::RealWork`](crate::palw_state_v2::PalwDeltaEntryV2) entry's `old` and the last one's `new` (a time step and
/// an event in one block are one move: Normal → Idle → Probe is reported as Normal → Probe), the default for `None`.
pub fn palw_floor_state_change_v1(entries: &[crate::palw_state_v2::PalwDeltaEntryV2]) -> Option<(PalwFloorStateV1, PalwFloorStateV1)> {
    let mut first_old = None;
    let mut last_new = None;
    for entry in entries {
        if let crate::palw_state_v2::PalwDeltaEntryV2::RealWork { old, new } = entry {
            first_old.get_or_insert_with(|| old.unwrap_or_default());
            last_new = Some(new.unwrap_or_default());
        }
    }
    let (from, to) = (first_old?, last_new?);
    (from != to).then_some((from, to))
}

/// **The entries a testnet-12 flag-day list takes to arm the Useful Work Transition's two consensus
/// fences** (ADR-0165), through their own `set`, which writes the bundle's mirror.
pub const PALW_T12_FLOOR_RESERVE_ENTRY: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_floor_reserve_v1",
    set: |params, at| {
        params.palw_floor_reserve_v1 = at;
        params.sync_palw_floor_reserve_v1();
    },
};
pub const PALW_T12_REAL_CLOCK_TICK_ENTRY: PalwPostLaunchFenceV1 =
    PalwPostLaunchFenceV1 { name: "palw_real_clock_tick_v1", set: |params, at| params.palw_real_clock_tick_v1 = at };

/// The values the fingerprint hashes beside the reserve fence's height, so changing a constant is a new network id.
pub fn palw_floor_reserve_value_v1() -> [u64; 3] {
    [PALW_FLOOR_IDLE_SLOTS_V1, PALW_FLOOR_PROBE_SLOTS_V1, PALW_FLOOR_PROBE_COOLDOWN_SLOTS_V1]
}

/// **The heartbeat miner's grace** (node policy, not consensus): after a slot opens it waits this long
/// for an attempt-lane block to carry the tick before it mints a heartbeat. 20 s is a sixth of the
/// interval, so a network with no real producer ticks every 120 + 20 s at the worst.
pub const PALW_REAL_TICK_GRACE_MS_V1: u64 = 20_000;

/// **What a mergeset holds, as the clock reads it** — the counts and newest stamps `palw_clock_step_v1`
/// gathers from the headers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwClockMergesetFactsV1 {
    /// Blocks `bits` prices (any one of them is the clock's own source and no tick source is needed).
    pub priced: u64,
    pub heartbeats: u64,
    pub attempts: u64,
    pub newest_beat_ms: Option<u64>,
    pub newest_attempt_ms: Option<u64>,
}

/// **The tick source rule, one function** (ADR-0165 B; read by `palw_clock_step_v1`, asked by the tests):
/// `(stand_in, source_ms)` — whether the mergeset has a tick source, and the stamp the cursor's slot is
/// measured against (the NEWEST source's; `0` where there is none). With `attempt_ticks` false it is the
/// heartbeat-only rule (ADR-0138 §3b), byte for byte; with it true an attempt-lane block is a source
/// beside the heartbeat. A mergeset has ONE tick however many sources it holds: the caller removes one
/// exemption, never one per source.
pub fn palw_clock_tick_source_v1(facts: &PalwClockMergesetFactsV1, attempt_ticks: bool) -> (bool, u64) {
    let beat = facts.priced == 0 && facts.heartbeats > 0;
    let attempt = attempt_ticks && facts.priced == 0 && facts.attempts > 0;
    let newest = if attempt_ticks { facts.newest_beat_ms.into_iter().chain(facts.newest_attempt_ms).max() } else { facts.newest_beat_ms };
    (beat || attempt, newest.unwrap_or(0))
}

/// **The release's list** (ADR-0165): the Useful Work Transition's two consensus fences, which the
/// DAA-5,300 flag day arms together (lane INT places the entries in its own list).
pub const PALW_T12_USEFUL_WORK_FENCES_V1: &[PalwPostLaunchFenceV1] = &[PALW_T12_FLOOR_RESERVE_ENTRY, PALW_T12_REAL_CLOCK_TICK_ENTRY];

/// The drill's list (`--palw-drill-useful-work-at`, [`crate::config::drill::palw_drill_useful_work_at_v1`]).
pub const PALW_DRILL_USEFUL_WORK_FENCES_V1: &[PalwPostLaunchFenceV1] = PALW_T12_USEFUL_WORK_FENCES_V1;

impl Params {
    /// `palw_floor_reserve_v1`, resolved: `Some` only on a `ConsensusV2` network that armed it.
    pub fn palw_floor_reserve_fence(&self) -> Option<ForkActivation> {
        match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(_) => self.palw_floor_reserve_v1.filter(|f| *f != ForkActivation::never()),
            _ => None,
        }
    }

    /// Is the floor a reserve at `daa_score`? `false` on every shipped preset.
    pub fn palw_floor_reserve_active_at(&self, daa_score: u64) -> bool {
        self.palw_floor_reserve_fence().is_some_and(|f| f.is_active(daa_score))
    }

    /// `palw_real_clock_tick_v1`, resolved.
    pub fn palw_real_clock_tick_fence(&self) -> Option<ForkActivation> {
        match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(_) => self.palw_real_clock_tick_v1.filter(|f| *f != ForkActivation::never()),
            _ => None,
        }
    }

    /// Do attempt-lane blocks carry the clock tick at `daa_score`? `false` on every shipped preset.
    pub fn palw_real_clock_tick_active_at(&self, daa_score: u64) -> bool {
        self.palw_real_clock_tick_fence().is_some_and(|f| f.is_active(daa_score))
    }

    /// **The reserve fence's mirror** on the V2 bundle's state params, which the fold reads.
    pub fn sync_palw_floor_reserve_v1(&mut self) {
        let from_daa = self.palw_floor_reserve_v1.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
        if let PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_floor_reserve_from_daa(from_daa);
        }
    }

    /// **The Useful Work Transition's refusals**, asked by [`Params::validate_palw_v2`]:
    ///
    /// * the reserve's mirror disagrees with the fence;
    /// * either fence off a `ConsensusV2` network;
    /// * the reserve without the model registry at or below it — "real class" is a registry fact (an
    ///   Active class other than the base), and a network without a registry has none;
    /// * the clock tick without the cursor, the floor, the lead cap and the single lottery at or below
    ///   it: it is a rule ABOUT the cursor's slots and the lead cap's bound, and an attempt is
    ///   unpriced by `bits` only past the single lottery, so before it an attempt in a mergeset is a
    ///   priced block and no tick source at all.
    pub fn validate_palw_useful_work_v1(&self) -> Result<(), PalwModeV2Error> {
        let mirror = match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(bundle) => bundle.state.floor_reserve_from_daa(),
            _ => None,
        };
        let armed = self.palw_floor_reserve_v1.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
        if mirror != armed {
            return Err(PalwModeV2Error::Invalid(
                "palw_floor_reserve_v1 disagrees with the V2 bundle's mirror: mirror it with Params::sync_palw_floor_reserve_v1",
            ));
        }
        let below = |fence: Option<ForkActivation>, at: u64| fence.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= at);
        if let Some(at) = armed {
            if !matches!(self.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)) {
                return Err(PalwModeV2Error::Invalid("palw_floor_reserve_v1 is a ConsensusV2 rule: this ruleset has no V2 bundle"));
            }
            if !below(self.palw_model_registry, at) {
                return Err(PalwModeV2Error::Invalid(
                    "palw_floor_reserve_v1 needs palw_model_registry at or below it: a real class is a registry fact",
                ));
            }
        }
        if let Some(at) = self.palw_real_clock_tick_v1.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score()) {
            if !matches!(self.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)) {
                return Err(PalwModeV2Error::Invalid("palw_real_clock_tick_v1 is a ConsensusV2 rule: this ruleset has no V2 bundle"));
            }
            if !(below(self.palw_clock_cursor, at)
                && below(self.palw_clock_floor, at)
                && below(self.palw_clock_lead_cap, at)
                && below(self.palw_single_lottery, at)
                && below(self.palw_anchor_clock, at))
            {
                return Err(PalwModeV2Error::Invalid(
                    "palw_real_clock_tick_v1 needs palw_anchor_clock, palw_single_lottery, palw_clock_cursor, palw_clock_floor and \
                     palw_clock_lead_cap at or below it: it adds a tick source to the cursor's slots, under the floor's stamp rules \
                     and the lead cap",
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_clock_cursor_v1::{palw_clock_cursor_from_reference_v1, palw_clock_slot_admits_v1};
    use crate::palw_heartbeat_v1::HEARTBEAT_RECOVERY_INTERVAL_MS as I;

    fn facts(heartbeats: u64, attempts: u64, beat: Option<u64>, attempt: Option<u64>) -> PalwClockMergesetFactsV1 {
        PalwClockMergesetFactsV1 { priced: 0, heartbeats, attempts, newest_beat_ms: beat, newest_attempt_ms: attempt }
    }

    #[test]
    fn without_the_fence_an_attempt_is_no_tick_source_and_with_it_the_newest_source_decides() {
        let f = facts(0, 7, None, Some(500));
        assert_eq!(palw_clock_tick_source_v1(&f, false), (false, 0), "heartbeat-only below the fence");
        assert_eq!(palw_clock_tick_source_v1(&f, true), (true, 500));
        let both = facts(2, 100, Some(900), Some(400));
        assert_eq!(palw_clock_tick_source_v1(&both, true), (true, 900), "the newest of either kind");
        assert_eq!(palw_clock_tick_source_v1(&both, false), (true, 900));
        let priced = PalwClockMergesetFactsV1 { priced: 1, ..f };
        assert_eq!(palw_clock_tick_source_v1(&priced, true).0, false, "a bits-priced block is the clock; no stand-in");
    }

    /// **The clock-safety simulation (ADR-0165 §3.4).** A chain of steps, each merging every tick source
    /// that arrived since the last one; sources are heartbeats or attempts stamped by an adversary anywhere
    /// from `now` to `now + 132 s`, in bursts of up to 100 at one instant. A step is stamped at
    /// `max(now, slot)` and may be withheld by the adversary for a while. Whatever it does, with the
    /// fence ON: (1) the DAA advances at most ONCE per step, whatever the burst; (2) ticks are never
    /// closer than one interval in stamp; (3) over any horizon the DAA has advanced at most
    /// `horizon / interval + 2` — what a heartbeat-only miner can do, and the burst bound `⌊132/120⌋ + 1`.
    #[test]
    fn a_producer_of_any_mix_cannot_run_the_clock_faster_than_a_heartbeat_miner() {
        let mut seed = 0xC10C_u64;
        let mut next = move || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            seed >> 33
        };
        for attempt_ticks in [false, true] {
            for _round in 0..300 {
                let start = 10_000_000u64;
                let mut now = start;
                let mut reference = start; // the stamp of the block that last advanced the score
                let mut daa = 0u64;
                let mut last_tick_stamp = start;
                let steps = 40 + next() % 80;
                for _ in 0..steps {
                    now += next() % (2 * I);
                    // a burst of sources at this instant: counts and stamps as the adversary likes
                    let n_beats = next() % 4;
                    let n_attempts = next() % 101;
                    let stamp = |r: u64| now + r % 132_001;
                    let newest_beat = (n_beats > 0).then(|| (0..n_beats).map(|_| stamp(next())).max().unwrap());
                    let newest_attempt = (n_attempts > 0).then(|| (0..n_attempts).map(|_| stamp(next())).max().unwrap());
                    let f = facts(n_beats, n_attempts, newest_beat, newest_attempt);
                    let (stand_in, source_ms) = palw_clock_tick_source_v1(&f, attempt_ticks);
                    let cursor = palw_clock_cursor_from_reference_v1(reference, I);
                    let granted = stand_in && palw_clock_slot_admits_v1(&cursor, source_ms).is_ok();
                    if granted {
                        // H5 + the lead cap: the step is stamped at or past its slot and not past now + 132 s.
                        let step_stamp = now.max(cursor.next_slot_ms);
                        if step_stamp > now + 132_000 {
                            continue; // refused by the lead cap: not merged now
                        }
                        daa += 1;
                        assert!(step_stamp >= last_tick_stamp, "stamps of ticks never go back");
                        assert!(step_stamp - reference >= I, "two ticks are at least one interval apart in stamp");
                        last_tick_stamp = step_stamp;
                        reference = step_stamp;
                    }
                }
                let horizon = (now - start).max(1);
                assert!(daa <= horizon / I + 2, "attempt_ticks={attempt_ticks}: {daa} ticks over {horizon} ms (≤ {})", horizon / I + 2);
            }
        }
    }

    /// One tick however many attempts: the exemption count a mergeset leaves is `exempt − 1` for any
    /// number of attempts (the arithmetic `palw_clock_step_v1` performs on `granted`).
    #[test]
    fn a_hundred_attempts_in_one_slot_advance_the_score_by_one() {
        let cursor = palw_clock_cursor_from_reference_v1(1_000, I);
        for attempts in [1u64, 2, 100, 10_000] {
            let f = facts(0, attempts, None, Some(1_000 + I));
            let (stand_in, ms) = palw_clock_tick_source_v1(&f, true);
            let granted = stand_in && palw_clock_slot_admits_v1(&cursor, ms).is_ok();
            let exempt = attempts; // every attempt is exempt past the single lottery
            let after = if granted { exempt.saturating_sub(1) } else { exempt };
            assert_eq!(exempt - after, 1, "{attempts} attempts, one tick");
        }
    }

    // ---- the floor state machine: ONE pure function ----

    use PalwFloorModeV1::{Idle, Normal, Probe};
    const IDLE: u64 = PALW_FLOOR_IDLE_SLOTS_V1;
    const PROBE: u64 = PALW_FLOOR_PROBE_SLOTS_V1;
    const COOL: u64 = PALW_FLOOR_PROBE_COOLDOWN_SLOTS_V1;

    fn st(mode: PalwFloorModeV1, last_probe_end: Option<u64>) -> PalwFloorStateV1 {
        PalwFloorStateV1 { mode, last_probe_end }
    }
    fn blue() -> Option<PalwRealAcceptedV1> {
        Some(PalwRealAcceptedV1 { blue: true })
    }
    fn red() -> Option<PalwRealAcceptedV1> {
        Some(PalwRealAcceptedV1 { blue: false })
    }

    /// **Every arc of the machine**, as a table: (state, DAA, event) → state.
    #[test]
    fn every_arc_of_the_floor_state_machine() {
        let idle = PalwFloorStateV1::default();
        // Idle, cooldown run (or no probe ever): a RED REAL attempt starts a Probe…
        assert_eq!(palw_floor_step_v1(idle, 100, red()), st(Probe { until: 100 + PROBE }, None));
        // …and a BLUE one is Normal at once: verified success needs no probe.
        assert_eq!(palw_floor_step_v1(idle, 100, blue()), st(Normal { last_blue: 100 }, None), "BLUE in Idle: Normal directly");
        // Idle, cooldown NOT run: a RED one changes nothing; a BLUE one is Normal all the same — the cooldown is for RED-only
        // streams, never for success.
        let cooling = st(Idle, Some(100));
        for daa in [100, 100 + COOL - 1] {
            assert_eq!(palw_floor_step_v1(cooling, daa, red()), cooling, "daa {daa}: the cooldown holds a RED one off");
            assert_eq!(
                palw_floor_step_v1(cooling, daa, blue()),
                st(Normal { last_blue: daa }, Some(100)),
                "daa {daa}: a BLUE one is Normal whatever the cooldown, and leaves it where it was"
            );
        }
        assert_eq!(palw_floor_step_v1(cooling, 100 + COOL, red()), st(Probe { until: 100 + COOL + PROBE }, Some(100)), "and runs at exactly COOL");
        // Probe: RED changes nothing, BLUE makes it Normal; the probe's own end writes `last_probe_end`.
        let probe = st(Probe { until: 106 }, Some(7));
        assert_eq!(palw_floor_step_v1(probe, 103, red()), probe, "RED never extends a probe");
        assert_eq!(palw_floor_step_v1(probe, 105, blue()), st(Normal { last_blue: 105 }, Some(7)), "a BLUE inside it is Normal");
        assert_eq!(palw_floor_step_v1(probe, 105, None), probe, "inside the probe, time changes nothing");
        assert_eq!(palw_floor_step_v1(probe, 106, None), st(Idle, Some(106)), "at `until` it expires unanswered, recording its end");
        assert_eq!(palw_floor_step_v1(probe, 150, None), st(Idle, Some(106)), "the nominal end, not the DAA it was noticed at");
        // …and an event at the very slot the probe expires meets Idle: a RED one is held off by the cooldown, a BLUE one is
        // Normal.
        assert_eq!(palw_floor_step_v1(probe, 106, red()), st(Idle, Some(106)), "a RED at `until`: Idle, cooling");
        assert_eq!(palw_floor_step_v1(probe, 106, blue()), st(Normal { last_blue: 106 }, Some(106)), "a BLUE at `until` is Normal");
        // Normal: BLUE refreshes, RED changes nothing, and it outlives its last BLUE by `floor_idle_slots`.
        let normal = st(Normal { last_blue: 200 }, None);
        assert_eq!(palw_floor_step_v1(normal, 210, blue()), st(Normal { last_blue: 210 }, None));
        assert_eq!(palw_floor_step_v1(normal, 210, red()), normal, "RED never extends Normal");
        assert_eq!(palw_floor_step_v1(normal, 200 + IDLE, None), normal, "still Normal at exactly floor_idle_slots");
        assert_eq!(palw_floor_step_v1(normal, 200 + IDLE + 1, None), st(Idle, None), "Idle one slot later");
        assert_eq!(palw_floor_step_v1(normal, 200 + IDLE + 1, blue()), st(Normal { last_blue: 200 + IDLE + 1 }, None), "a BLUE after that is Normal again");
        assert_eq!(palw_floor_step_v1(normal, 200 + IDLE + 1, red()), st(Probe { until: 200 + IDLE + 1 + PROBE }, None), "a RED after that probes");
        // A Normal that ended leaves `last_probe_end` where it was.
        assert_eq!(palw_floor_step_v1(st(Normal { last_blue: 50 }, Some(7)), 300, None), st(Idle, Some(7)));
    }

    /// **Time is path independent**: stepping to `t1` and then `t2` is stepping to `t2` — so the fold may advance at every
    /// block, and a reader may advance lazily, and they agree.
    #[test]
    fn time_is_idempotent_and_path_independent() {
        let mut seed = 0xF100_u64;
        let mut next = move || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            seed >> 33
        };
        for _ in 0..20_000 {
            let base = next() % 1_000;
            let mode = match next() % 3 {
                0 => Idle,
                1 => Probe { until: base + next() % 10 },
                _ => Normal { last_blue: base + next() % 40 },
            };
            let state = st(mode, (next() % 2 == 0).then(|| base + next() % 30));
            let (t1, t2) = (base + next() % 80, base + 80 + next() % 80);
            let direct = palw_floor_step_v1(state, t2, None);
            assert_eq!(palw_floor_step_v1(palw_floor_step_v1(state, t1, None), t2, None), direct, "{state:?} at {t1} then {t2}");
            assert_eq!(palw_floor_step_v1(direct, t2, None), direct, "idempotent");
            assert_eq!(state.admits_floor_at(t2), direct.mode == Idle);
        }
    }

    /// **Several attempts in one block apply in the fold's order**, each on the state the one before left.
    #[test]
    fn several_attempts_in_one_slot_apply_in_the_folds_order() {
        let run = |events: &[Option<PalwRealAcceptedV1>]| events.iter().fold(PalwFloorStateV1::default(), |s, e| palw_floor_step_v1(s, 500, *e));
        assert_eq!(run(&[red(), blue()]), st(Normal { last_blue: 500 }, None), "Idle + [RED, BLUE]: Probe, then Normal");
        assert_eq!(run(&[blue(), red()]), st(Normal { last_blue: 500 }, None), "Idle + [BLUE, RED]: Normal, and the RED changes nothing");
        assert_eq!(run(&[blue(), blue()]), st(Normal { last_blue: 500 }, None), "Idle + [BLUE, BLUE]: Normal");
        assert_eq!(run(&[red(), red(), red()]), st(Probe { until: 500 + PROBE }, None), "a burst of REDs is one probe");
    }

    /// **A RED-only stream cannot keep the floor refused beyond `probe_slots` per cooldown**: a RED REAL attempt in EVERY
    /// slot for 3,000 slots leaves the floor refused in at most `probe_slots` of any `probe_slots + probe_cooldown_slots`
    /// consecutive slots, and never for a longer run than `probe_slots`.
    #[test]
    fn a_red_only_stream_cannot_keep_the_floor_refused_beyond_probe_slots_per_cooldown() {
        for every in [1u64, 2, 3, 5, 10, 17, 26, 40] {
            let mut state = PalwFloorStateV1::default();
            let mut refused = Vec::new();
            for daa in 0..3_000u64 {
                state = palw_floor_step_v1(state, daa, if daa % every == 0 { red() } else { None });
                refused.push(!state.admits_floor_at(daa));
            }
            let window = (PROBE + COOL) as usize;
            let worst = refused.windows(window).map(|w| w.iter().filter(|r| **r).count()).max().unwrap();
            assert!(worst as u64 <= PROBE, "a RED every {every} slots: {worst} refused slots in {window}");
            let mut run = 0u64;
            let mut longest = 0u64;
            for r in &refused {
                run = if *r { run + 1 } else { 0 };
                longest = longest.max(run);
            }
            assert!(longest <= PROBE, "a RED every {every} slots: a run of {longest} refused slots");
            assert!(refused.iter().any(|r| *r), "{every}: and the stream does open probes");
        }
    }

    /// **BLUE REAL attempts sustain Normal, and Normal ends `floor_idle_slots` after the last** — and a stream that lands
    /// BLUE every few slots keeps the floor refused without a gap, from its very first attempt.
    #[test]
    fn blue_attempts_sustain_normal_and_it_ends_floor_idle_slots_after_the_last() {
        let mut state = PalwFloorStateV1::default();
        let mut daa = 0u64;
        // The 8k stream: a BLUE attempt every 3 slots. The first is Normal at once.
        for i in 0..40 {
            state = palw_floor_step_v1(state, daa, blue());
            assert!(!state.admits_floor_at(daa), "attempt {i} at DAA {daa}: the floor is refused");
            assert_eq!(state.mode, Normal { last_blue: daa });
            daa += 3;
        }
        let last = daa - 3;
        for later in last..=last + IDLE {
            assert!(!state.admits_floor_at(later), "{} slots after the last BLUE: still refused", later - last);
        }
        assert!(state.admits_floor_at(last + IDLE + 1), "floor_idle_slots + 1 slots after it the floor is the fallback again");
    }

    /// **A BLUE attempt that lands after a probe expired is Normal all the same** — the cooldown holds RED-only streams
    /// off, not success: a probe that nobody answered in time does not silence a BLUE attempt that arrives just after it.
    #[test]
    fn a_blue_attempt_after_the_probe_expired_is_normal_even_while_the_cooldown_runs() {
        let mut state = PalwFloorStateV1::default();
        let until = 100 + PROBE;
        state = palw_floor_step_v1(state, 100, red()); // the first REAL attempt, RED: a probe through `until`
        state = palw_floor_step_v1(state, until, None);
        assert_eq!(state, st(Idle, Some(until)));
        assert!(state.admits_floor_at(until), "the floor is valid from `until`");
        state = palw_floor_step_v1(state, until + 2, blue()); // BLUE, a little late
        assert_eq!(state, st(Normal { last_blue: until + 2 }, Some(until)), "Normal, with the cooldown's reference untouched");
        assert!(!state.admits_floor_at(until + 2), "and the floor is refused again");
        // A RED one in the cooldown, by contrast, is held off.
        let idle = st(Idle, Some(until));
        assert_eq!(palw_floor_step_v1(idle, until + 2, red()), idle);
        assert_eq!(
            palw_floor_step_v1(idle, until + COOL, red()).mode,
            Probe { until: until + COOL + PROBE },
            "and opens the next probe once it has run"
        );
    }

    /// **The constants are the measured ones, derived and bounded.**
    #[test]
    fn the_constants_are_the_measured_ones_and_bounded() {
        assert_eq!(PALW_REAL_MEASURED_GAP_P95_MS_V1, 17_300 * PALW_REAL_SLOT_MS_V1 / 1_000, "17.3 slots");
        assert_eq!(
            palw_floor_idle_slots_for_v1(PALW_REAL_MEASURED_GAP_P95_MS_V1, PALW_FLOOR_IDLE_MARGIN_SLOTS_V1),
            PALW_FLOOR_IDLE_SLOTS_V1,
            "ceil(17.3) + 2 = 20: floor_idle_slots is the measured gap with its margin"
        );
        assert_eq!(PALW_FLOOR_IDLE_SLOTS_V1, 20);
        assert_eq!(
            palw_floor_probe_slots_for_v1(PALW_REAL_MEASURED_DRAW_P50_MS_V1, PALW_REAL_MEASURED_DRAW_MAX_MS_V1, 1),
            PALW_FLOOR_PROBE_SLOTS_V1,
            "ceil(325 s / 120 s) = 3, + ceil(426 s / 120 s) = 4, + one slot of accepting-block lag = 8"
        );
        assert_eq!(PALW_FLOOR_PROBE_SLOTS_V1, 8);
        assert_eq!(PALW_FLOOR_PROBE_COOLDOWN_SLOTS_V1, 20);
        // The derivation on other inputs: a rounding up, a floor of one, a ceiling.
        assert_eq!(palw_floor_idle_slots_for_v1(0, 0), 1, "never below one slot");
        assert_eq!(palw_floor_idle_slots_for_v1(PALW_REAL_SLOT_MS_V1 + 1, 0), 2, "rounds the gap up");
        assert_eq!(palw_floor_idle_slots_for_v1(u64::MAX, u64::MAX), PALW_FLOOR_IDLE_SLOTS_MAX_V1, "saturating, capped, never a panic");
        for gap in (0..40).map(|i| i * PALW_REAL_SLOT_MS_V1 / 2) {
            for margin in 0..4 {
                assert!(palw_floor_idle_slots_for_v1(gap, margin) <= palw_floor_idle_slots_for_v1(gap + PALW_REAL_SLOT_MS_V1, margin));
                assert!(palw_floor_idle_slots_for_v1(gap, margin) <= palw_floor_idle_slots_for_v1(gap, margin + 1));
            }
        }
        // The ceiling is an hour — the merge-depth duration — and the shipped constants sit inside it and in the fingerprint.
        assert_eq!(PALW_FLOOR_IDLE_SLOTS_MAX_V1 * PALW_REAL_SLOT_MS_V1 / 1_000, crate::config::constants::consensus::MERGE_DEPTH_DURATION);
        assert!((1..=PALW_FLOOR_IDLE_SLOTS_MAX_V1).contains(&PALW_FLOOR_IDLE_SLOTS_V1));
        assert_eq!(palw_floor_reserve_value_v1(), [PALW_FLOOR_IDLE_SLOTS_V1, PALW_FLOOR_PROBE_SLOTS_V1, PALW_FLOOR_PROBE_COOLDOWN_SLOTS_V1]);
    }

    /// The state's rooted form: the default is `None`; every other state round-trips borsh; it prints its mode.
    #[test]
    fn the_state_has_a_canonical_rooted_form_and_prints_its_mode() {
        assert_eq!(PalwFloorStateV1::default().canonical(), None);
        for state in [st(Idle, Some(5)), st(Probe { until: 9 }, None), st(Normal { last_blue: 3 }, Some(1))] {
            assert_eq!(state.canonical(), Some(state));
            let bytes = borsh::to_vec(&state).expect("encodes");
            assert_eq!(PalwFloorStateV1::try_from_slice(&bytes).expect("decodes"), state);
        }
        assert_eq!(PalwFloorStateV1::default().to_string(), "idle");
        assert!(st(Probe { until: 9 }, None).to_string().contains("probe") && st(Normal { last_blue: 3 }, None).to_string().contains("normal"));
    }

    /// **The floor-state line's format is pinned** — an operator and the drill grep it.
    #[test]
    fn the_floor_state_line_has_a_stable_prefix_and_names_the_move() {
        let line = |from: PalwFloorStateV1, to: PalwFloorStateV1| palw_floor_state_log_line_v1(&"b7", 4_242, &from, &to);
        assert_eq!(
            line(st(Idle, None), st(Probe { until: 4_248 }, None)),
            "[palw-floor-state] daa=4242 block=b7 idle->probe last_blue=- until=4248 last_probe_end=-"
        );
        assert_eq!(
            line(st(Probe { until: 4_248 }, None), st(Normal { last_blue: 4_242 }, None)),
            "[palw-floor-state] daa=4242 block=b7 probe->normal last_blue=4242 until=- last_probe_end=-"
        );
        assert_eq!(
            line(st(Normal { last_blue: 4_200 }, Some(3_000)), st(Idle, Some(3_000))),
            "[palw-floor-state] daa=4242 block=b7 normal->idle last_blue=- until=- last_probe_end=3000"
        );
        assert!(line(st(Idle, None), st(Idle, Some(1))).starts_with(PALW_FLOOR_STATE_LOG_TAG_V1));
        assert_eq!(PALW_FLOOR_STATE_LOG_TAG_V1, "[palw-floor-state]");
    }

    /// **One block's several writes are one move** — the first entry's `old` to the last one's `new`; nothing when the state ended
    /// where it began; the default stands in for `None`; entries of other kinds are ignored.
    #[test]
    fn a_blocks_net_floor_move_is_its_first_old_to_its_last_new() {
        use crate::palw_state_v2::PalwDeltaEntryV2 as E;
        let rw = |old: Option<PalwFloorStateV1>, new: Option<PalwFloorStateV1>| E::RealWork { old, new };
        assert_eq!(palw_floor_state_change_v1(&[]), None, "no entry, no move");
        assert_eq!(
            palw_floor_state_change_v1(&[rw(None, Some(st(Probe { until: 9 }, None)))]),
            Some((st(Idle, None), st(Probe { until: 9 }, None))),
            "None is the default"
        );
        // Time then event in one block: Normal -> Idle -> Probe is reported as Normal -> Probe.
        let normal = st(Normal { last_blue: 100 }, None);
        let idle = st(Idle, None);
        let probe = st(Probe { until: 127 }, None);
        assert_eq!(palw_floor_state_change_v1(&[rw(Some(normal), None), rw(None, Some(probe))]), Some((normal, probe)));
        // A move that ends where it began is no move.
        assert_eq!(palw_floor_state_change_v1(&[rw(Some(normal), Some(idle)), rw(Some(idle), Some(normal))]), None);
    }

    /// **The refusal's words are the ones the drill greps** — `FloorNotIdle`'s message carries the marker, and the producer's text
    /// test recognises exactly it.
    #[test]
    fn the_floor_refusal_carries_the_marker_the_producer_and_the_drill_read() {
        let refusal = crate::palw_state_v2::PalwStateV2Error::FloorNotIdle {
            class: crate::Hash64::default(),
            daa: 4_242,
            state: st(Probe { until: 4_248 }, None),
        }
        .to_string();
        assert!(refusal.contains(PALW_FLOOR_NOT_IDLE_MARKER_V1), "{refusal}");
        assert!(palw_is_floor_not_idle_refusal_v1(&refusal));
        assert!(refusal.contains("probe") && refusal.contains("4242"), "it names the state and the DAA: {refusal}");
        assert!(!palw_is_floor_not_idle_refusal_v1("class … is Held under the model registry"));
        assert!(!palw_is_floor_not_idle_refusal_v1(""));
    }
}
