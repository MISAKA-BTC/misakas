//! **Court capacity (audit G-07; user direction 2026-10-16, `docs/design/palw/t12-daa14000-release-plan-2026-10-15.md`
//! "court の全体容量と Panel 担保" §1)** — dormant behind `Params::palw_court_capacity_v1`, which no network can arm until a measured
//! record exists.
//!
//! The network's court capacity was a fixed concurrency cap (`palw_max_concurrent_court_sessions_v1`: the rung window times the ONE
//! court move a block admits — 42 sessions on testnet-12). A coalition with enough bonds could hold every slot and keep refilling it,
//! so "one honest verifier can prosecute" was not a capacity guarantee. Past this fence:
//!
//! ```text
//! court admission   a new court session needs the challenger's own LIVE LG14-A reservation on the claim (its claim-fixed hard
//!                   deadline, its Final hold), an Active bond at or above the floor, and the occupancy escrow on its free half
//! block budget      every court operation costs deterministic units (work, evidence bytes, memory — never runtime CPU time); a
//!                   block admits court operations up to the policy's budget; capacity = ⌊budget.work / max op work⌋ × rung window
//! time price        occupancy fee = ρ_court × occupied DAA × resource weight, escrowed for the maximum duration up front, the unused
//!                   part refunded at the end; a prosecution that convicts is reimbursed from the defendant's collateral first
//! scheduler         a session opens at once only while capacity is free and NOBODY is queued; otherwise a reserved prosecution
//!                   QUEUES. Freed capacity is granted to the queue in deadline order (earliest claim hard deadline first) at the
//!                   start of the next block, before any object — so a coalition re-filling freed slots never races an honest
//!                   queued prosecution. A prosecution is queued only if worst-case service accounting (every occupant held to its
//!                   backstop, every entry ahead held for its ticket window plus its maximum session) shows that its session, held to
//!                   its maximum, ends — its verdict lands — by its claim's hard deadline, WITHOUT breaking that guarantee for any
//!                   entry already queued; a session opened directly must likewise end by the hard deadline (the court term of
//!                   LG14-A's hard-deadline envelope)
//! ```
//!
//! **Placeholders.** The block budget, the per-operation work units, ρ_court and the weight unit are POLICY VALUES that must come from
//! measurement on the slowest supported node (`palw_court_capacity_placeholder_policy_v1` reproduces today's one-move-per-block rule
//! and prices nothing). Arming is refused on every real network until an arm-readiness record is pinned, and a record cannot arm the
//! placeholder policy.
//!
//! **Fused leaves.** Until this fence is armed, the fused-leaf adjudication route (KernelWitness → ShardCourtAccused → held
//! dissection) is not counted as a G14 route under option (b): `crate::palw_g14_gates_v1::PalwG14RouteFamilyV1::LegacyFusedLeaf`.
//!
//! **Round 2 (lane g14-court-capacity-r2, 2026-10-17; audit 2026-10-17 §6.1 / §7 P0(1), appendix E.1 limits 1–3).** Same fence:
//!
//! ```text
//! one-move budget   a one-move accusation (OneMove class) is charged to its OWN per-block budget (`one_move_budget`), never to the
//!                   session budget, and a session operation never to it: the session capacity is derived from a budget no one-move
//!                   accusation can spend (limit 3)
//! reserve pool      `reserve_permille` of the capacity is RESERVED for the first court row of each court-demanding claim (a
//!                   fused-attention V2 Attempt accepted past the fence — its only adjudication route is a court session): its claim's
//!                   reserve, one per claim over its life. A reserve row is never refused for capacity: it opens on a free reserve slot
//!                   or queues, and the reserve queue is granted earliest hard deadline first. Every other row is a SPARE row on the
//!                   rest of the capacity, with the round-1 deadline guarantee — and when that guarantee fails it is admitted
//!                   BEST-EFFORT (queued behind every guaranteed row) instead of refused: no prosecution is refused for capacity
//! bounded admission a new court-demanding claim is admitted only while its hard-deadline bucket (width W = ticket window + the network
//!                   court window) holds fewer than M = ⌊reserve slots / 2⌋ in-flight court-demanding claims; otherwise its attempt is
//!                   skipped (`CourtClaimAdmissionFull`: no claim, no reward — the reward's liveness degrades, safety does not). With
//!                   it, every claim's reserve row filed by `hard − 4W` reaches its verdict by `hard` (EDF on the reserve slots; the
//!                   demand-bound argument is in the design note, g14-completeness-proof §11) — whatever the coalition holds, queues
//!                   or refills on other claims (limit 1)
//! decoy price       the j-th concurrent court row on one claim escrows and pays j × the occupancy rate (limit 2: decoys on one claim
//!                   are priced by their number; a conviction still reimburses the honest prosecutor from the producer first)
//! unserved = unpaid a pre-Final claim whose live reservation still has a WAITING court row (queued or ticketed) at its lapse, admitted
//!                   at least one service window (ticket + session) before the hard deadline, is voided `PanelUnavailable` instead of
//!                   released toward Final: no reward, no weight, no credit, nobody slashed. A coalition that keeps an honest
//!                   prosecution from its verdict therefore gets nothing for the claim, and pays the occupancy of every slot it held
//! ```
//!
//! No cryptographic computation proof of any kind is used (PRINCIPLES §8): everything here is accounting over public state.

use borsh::{BorshDeserialize, BorshSerialize};

use crate::Hash64;
use crate::config::params::{ForkActivation, Params};
use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};
use crate::palw_state_v2::{PalwBondKeyV2, PalwChainStateV2, PalwConsensusObjectV2};

// =================================================================================================================================
// 1. Deterministic cost units
// =================================================================================================================================

/// **What one court operation costs a validating node, in deterministic units** — never runtime CPU time. `work` is the operation
/// class's measured work units, `bytes` its encoded evidence size, `memory` its working set in bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
pub struct PalwCourtCostV1 {
    pub work: u64,
    pub bytes: u64,
    pub memory: u64,
}

impl PalwCourtCostV1 {
    pub fn saturating_add(self, other: Self) -> Self {
        Self {
            work: self.work.saturating_add(other.work),
            bytes: self.bytes.saturating_add(other.bytes),
            memory: self.memory.saturating_add(other.memory),
        }
    }

    /// Every component within `budget`.
    pub fn fits_within(&self, budget: &Self) -> bool {
        self.work <= budget.work && self.bytes <= budget.bytes && self.memory <= budget.memory
    }
}

/// **The court operation classes the cost model distinguishes.** One work-unit entry each in the policy (`op_work`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PalwCourtOpClassV1 {
    /// A one-move accusation adjudicated whole at acceptance (`ShardCourtAccused`, `CheckpointAccused`, the IR / pipeline / GDN
    /// one-moves, LG14-B's leaf recompute): a leaf recompute from committed inputs and model rows.
    OneMove = 0,
    /// A bisection court opened (`CourtOpened`).
    Opening = 1,
    /// A dissection move (root claim, dissected round, child chosen — attention, IR, generative, evaluation).
    DissectionMove = 2,
    /// A court close (`CourtClosed`, `CourtClosedChunk`).
    Close = 3,
}

/// The number of operation classes (`op_work`'s length).
pub const PALW_COURT_OP_CLASSES_V1: usize = 4;

/// **Which court operation `object` is on `state`** (the state BEFORE it applies — the fold's builder state and the acceptance walk's
/// folded state are the same), or `None` for every object the court budget does not govern. The one-move accusations (adjudicated
/// whole at acceptance: a leaf recompute) and a court's opening are charged by variant; a dissection move, a split close and a terminal
/// evidence part are charged exactly when the shipped slot rule says they spend the block's adjudication slot
/// (`palw_court_move_spends_the_slot_v1`, `palw_court_close_completes_a_group_v1`) — so the work the slot bounded below the fence is the
/// work the budget bounds past it, and nothing the slot rule charged goes free.
pub fn palw_court_op_class_v1(state: &PalwChainStateV2, object: &PalwConsensusObjectV2) -> Option<PalwCourtOpClassV1> {
    use PalwConsensusObjectV2 as O;
    match object {
        O::ShardCourtAccused { .. }
        | O::CheckpointAccused { .. }
        | O::TirShardCourtAccused { .. }
        | O::GenShardCourtAccused { .. }
        | O::GdnStepAccusedV2 { .. }
        | O::LegacyLeafRecomputedV2 { .. } => Some(PalwCourtOpClassV1::OneMove),
        O::CourtOpened { .. } => Some(PalwCourtOpClassV1::Opening),
        O::CourtClosed { .. } => Some(PalwCourtOpClassV1::Close),
        _ if crate::palw_state_v2::palw_court_close_completes_a_group_v1(state, object) => Some(PalwCourtOpClassV1::Close),
        _ if crate::palw_state_v2::palw_court_move_spends_the_slot_v1(state, object) => Some(PalwCourtOpClassV1::DissectionMove),
        _ => None,
    }
}

/// **The deterministic cost of `object` on `state` under `policy`** — its class's work units, the evidence bytes it makes a node
/// process (its encoded size; for the chunk that completes a split close, the whole assembly it triggers), and its working set (those
/// bytes × the policy's memory factor). `None` for an object the court budget does not govern.
pub fn palw_court_op_cost_v1(
    policy: &PalwCourtCapacityPolicyV1,
    state: &PalwChainStateV2,
    object: &PalwConsensusObjectV2,
) -> Option<PalwCourtCostV1> {
    let class = palw_court_op_class_v1(state, object)?;
    let mut bytes = borsh::to_vec(object).map(|b| b.len() as u64).unwrap_or(u64::MAX);
    if let PalwConsensusObjectV2::CourtCloseChunk { session_id, side, .. } = object
        && let Some(group) = state.court_close_group(session_id, *side)
    {
        bytes = group.chunks.values().fold(bytes, |sum, chunk| sum.saturating_add(chunk.len() as u64));
    }
    Some(PalwCourtCostV1 { work: policy.op_work[class as usize], bytes, memory: bytes.saturating_mul(policy.memory_per_byte) })
}

// =================================================================================================================================
// 2. The policy and the fence
// =================================================================================================================================

/// The policy's format version.
pub const PALW_COURT_CAPACITY_POLICY_VERSION_V1: u16 = 1;

/// **The court-capacity policy** — every value here is a measured value on a real network.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwCourtCapacityPolicyV1 {
    pub version: u16,
    /// What one block may spend on court operations.
    pub block_budget: PalwCourtCostV1,
    /// Work units per operation class (`PalwCourtOpClassV1` order).
    pub op_work: [u64; PALW_COURT_OP_CLASSES_V1],
    /// Working-set bytes per encoded byte.
    pub memory_per_byte: u64,
    /// **ρ_court**: sompi per occupied DAA per resource-weight unit.
    pub rho_sompi: u64,
    /// The resource weight of one session: the work units one of its moves costs at most (`max(op_work)` when 0).
    pub session_weight: u64,
    /// The window a granted ticket stays open for its prosecutor to file the accusation (DAA); 0 = the rung window
    /// (`turn_deadline_daa`, the time any mover has for one move).
    pub ticket_window_daa: u64,
    /// **Round 2: the per-block budget of the one-move accusations** (`PalwCourtOpClassV1::OneMove`), separate from `block_budget`.
    pub one_move_budget: PalwCourtCostV1,
    /// **Round 2: the share of the session capacity reserved for each court-demanding claim's first row**, per mille (1..=1000).
    pub reserve_permille: u16,
}

/// **The placeholder policy** — today's rule reproduced: one court operation a block whatever its size (`PALW_COURT_CLOSE_MAX_PER_BLOCK`,
/// so the derived capacity is the rung window, 42 sessions on testnet-12), nothing priced (ρ = 0), the ticket window the rung window.
/// NOT a measurement: a real network is refused it even with a pinned record.
pub fn palw_court_capacity_placeholder_policy_v1() -> PalwCourtCapacityPolicyV1 {
    PalwCourtCapacityPolicyV1 {
        version: PALW_COURT_CAPACITY_POLICY_VERSION_V1,
        block_budget: PalwCourtCostV1 { work: 1, bytes: u64::MAX, memory: u64::MAX },
        op_work: [1; PALW_COURT_OP_CLASSES_V1],
        memory_per_byte: 1,
        rho_sompi: 0,
        session_weight: 0,
        ticket_window_daa: 0,
        one_move_budget: PalwCourtCostV1 { work: 1, bytes: u64::MAX, memory: u64::MAX },
        reserve_permille: 500,
    }
}

impl PalwCourtCapacityPolicyV1 {
    /// The policy's identity (hashed into the params and schedule ids beside the height).
    pub fn id(&self) -> Hash64 {
        let mut s = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/court-capacity/policy/v1").to_state();
        s.update(&borsh::to_vec(self).expect("a policy serializes"));
        Hash64::from_bytes(s.finalize().as_bytes().try_into().expect("64 bytes"))
    }

    pub fn is_placeholder(&self) -> bool {
        *self == palw_court_capacity_placeholder_policy_v1()
    }

    /// Structural soundness (not approval): every unit at least one, the budget admits the heaviest single operation's work.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.version != PALW_COURT_CAPACITY_POLICY_VERSION_V1 {
            return Err("palw_court_capacity_v1: unknown policy version");
        }
        if self.op_work.contains(&0) || self.memory_per_byte == 0 {
            return Err("palw_court_capacity_v1: every operation class costs at least one work unit and memory scales with bytes");
        }
        if self.block_budget.work < self.session_op_work() || self.block_budget.bytes == 0 || self.block_budget.memory == 0 {
            return Err("palw_court_capacity_v1: the block budget must admit the heaviest single court operation");
        }
        if self.one_move_budget.work < self.op_work[PalwCourtOpClassV1::OneMove as usize]
            || self.one_move_budget.bytes == 0
            || self.one_move_budget.memory == 0
        {
            return Err("palw_court_capacity_v1: the one-move budget must admit one one-move accusation");
        }
        if self.reserve_permille == 0 || self.reserve_permille > 1_000 {
            return Err("palw_court_capacity_v1: the reserve share is 1..=1000 per mille");
        }
        Ok(())
    }

    /// **The block budget `class` is charged to**: a one-move accusation its own budget, every session operation the session budget.
    pub fn budget_of(&self, class: PalwCourtOpClassV1) -> PalwCourtCostV1 {
        if class == PalwCourtOpClassV1::OneMove { self.one_move_budget } else { self.block_budget }
    }

    /// **The session operations' budget work**, the one the capacity is derived from (`max_op_work` over the session classes).
    fn session_op_work(&self) -> u64 {
        self.op_work[1..].iter().copied().max().unwrap_or(1).max(1)
    }

    /// **The reserve slots** (`reserve_permille` of the capacity, at least one) at rung window `turn_deadline_daa`.
    pub fn reserve_slots(&self, turn_deadline_daa: u64) -> u64 {
        let capacity = self.capacity_sessions(turn_deadline_daa);
        (u128::from(capacity) * u128::from(self.reserve_permille) / 1_000).max(1).min(u128::from(capacity)) as u64
    }

    /// **The spare slots**: the capacity less the reserve.
    pub fn spare_slots(&self, turn_deadline_daa: u64) -> u64 {
        self.capacity_sessions(turn_deadline_daa).saturating_sub(self.reserve_slots(turn_deadline_daa))
    }

    /// **The bounded admission's bucket width W** (DAA): the ticket window plus the network court window — one reserve row's
    /// worst-case service.
    pub fn admission_bucket_daa(&self, turn_deadline_daa: u64, window_court: u64) -> u64 {
        self.ticket_window(turn_deadline_daa).saturating_add(window_court).max(1)
    }

    /// **M: the court-demanding claims one hard-deadline bucket admits** — half the reserve slots (the factor 2 absorbs the bucket
    /// alignment and the non-preemptive service; g14-completeness-proof §11).
    pub fn admission_claims_per_bucket(&self, turn_deadline_daa: u64) -> u64 {
        self.reserve_slots(turn_deadline_daa) / 2
    }

    /// **The lead a reserve row needs for the guarantee**: filed at or before `hard − 4W`, it reaches its verdict by `hard`.
    pub fn reserve_lead_daa(&self, turn_deadline_daa: u64, window_court: u64) -> u64 {
        self.admission_bucket_daa(turn_deadline_daa, window_court).saturating_mul(4)
    }

    /// The heaviest operation class's work units.
    pub fn max_op_work(&self) -> u64 {
        self.op_work.iter().copied().max().unwrap_or(1).max(1)
    }

    /// One session's resource weight.
    pub fn weight(&self) -> u64 {
        if self.session_weight == 0 { self.max_op_work() } else { self.session_weight }
    }

    /// The ticket window at a network whose rung window is `turn_deadline_daa`.
    pub fn ticket_window(&self, turn_deadline_daa: u64) -> u64 {
        if self.ticket_window_daa == 0 { turn_deadline_daa.max(1) } else { self.ticket_window_daa }
    }

    /// **The concurrent sessions the budget can serve**: every session needs at most one move of at most the heaviest session class's
    /// units per rung window, and a block admits `⌊budget.work / that⌋` of them — so `⌊budget.work / max_op_work⌋ × turn_deadline_daa` sessions
    /// each get their move inside their rung (the derivation `palw_max_concurrent_court_sessions_v1` makes for one move a block).
    pub fn capacity_sessions(&self, turn_deadline_daa: u64) -> u64 {
        // Round 2: a one-move accusation never spends `block_budget` (it has its own), so the capacity is the session classes'.
        (self.block_budget.work / self.session_op_work()).saturating_mul(turn_deadline_daa).max(1)
    }

    /// **The occupancy fee** for `occupied_daa` DAA at weight `weight`: ρ_court × occupied DAA × weight.
    pub fn fee(&self, occupied_daa: u64, weight: u64) -> u128 {
        u128::from(self.rho_sompi).saturating_mul(u128::from(occupied_daa)).saturating_mul(u128::from(weight))
    }
}

// =================================================================================================================================
// 2b. The measurement record and the policy it derives
// =================================================================================================================================

/// The measurement record's format version.
pub const PALW_COURT_MEASUREMENT_VERSION_V1: u16 = 1;

/// The index, in the court-capacity arm-readiness record's evidence, of the digest of its measurement record
/// (`PALW_COURT_CAPACITY_ARMING_REQUIREMENTS_V1[1]`, "measured: each court operation class's ...").
pub const PALW_COURT_MEASUREMENT_EVIDENCE_INDEX_V1: usize = 1;

/// **What the court-capacity policy is derived from** — the measurement record a network arms with (its digest is the arm-readiness
/// record's measured-values evidence, [`PALW_COURT_MEASUREMENT_EVIDENCE_INDEX_V1`]). Timings, sizes and working sets are those of the
/// SLOWEST supported node, release build, each operation class at its maximum profile (the `palw_court_op_timing_harness` and the node
/// E2E's `[court-op timing]` produce them on a given machine); the three block allotments and ρ_court are the network's approved
/// choices, reported beside them. No runtime timing ever enters consensus: the derived policy is integers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwCourtMeasurementV1 {
    pub version: u16,
    /// The worst verification time per operation class (`PalwCourtOpClassV1` order), in nanoseconds.
    pub class_ns: [u64; PALW_COURT_OP_CLASSES_V1],
    /// The worst encoded evidence size per class, in bytes.
    pub class_bytes: [u64; PALW_COURT_OP_CLASSES_V1],
    /// The worst peak working set per class, in bytes.
    pub class_memory: [u64; PALW_COURT_OP_CLASSES_V1],
    /// The court verification time one block may take on that node (nanoseconds): the share of the block interval given to courts.
    pub block_court_ns: u64,
    /// The court evidence bytes one block may carry.
    pub block_court_bytes: u64,
    /// The court working set one block may hold, in bytes.
    pub block_court_memory: u64,
    /// ρ_court: sompi per occupied DAA per work unit (the occupancy economics the record reports).
    pub rho_sompi: u64,
    /// The ticket window, DAA (0: the rung window).
    pub ticket_window_daa: u64,
    /// **Round 2: the one-move accusations' verification time one block may take** (nanoseconds), separate from `block_court_ns`.
    pub block_one_move_ns: u64,
    /// **Round 2: the one-move accusations' evidence bytes one block may carry.**
    pub block_one_move_bytes: u64,
    /// **Round 2: the reserve share** (per mille of the capacity; the network's approved choice).
    pub reserve_permille: u16,
}

impl PalwCourtMeasurementV1 {
    /// The digest the arm-readiness record carries for this measurement.
    pub fn digest(&self) -> [u8; 32] {
        let mut s = blake2b_simd::Params::new().hash_length(32).key(b"misaka-palw/court-capacity/measurement/v1").to_state();
        s.update(&borsh::to_vec(self).expect("a measurement serializes"));
        s.finalize().as_bytes().try_into().expect("32 bytes")
    }

    /// **The policy this measurement derives**, deterministically: the work unit is the cheapest class's time; each class costs
    /// `⌈its time / unit⌉` units; the block budget is `⌊block_court_ns / unit⌋` units, the block's byte and working-set allotments
    /// (the one-move budget likewise from `block_one_move_ns` / `block_one_move_bytes`, sharing the working-set allotment);
    /// the working set per encoded byte is the worst class's `⌈memory / bytes⌉`; the session weight is the heaviest class's units.
    /// Refused when a class is unmeasured (0) or the allotment cannot take the heaviest class once.
    pub fn policy(&self) -> Result<PalwCourtCapacityPolicyV1, &'static str> {
        if self.version != PALW_COURT_MEASUREMENT_VERSION_V1 {
            return Err("palw_court_capacity_v1: unknown measurement version");
        }
        if self.class_ns.contains(&0) || self.class_bytes.contains(&0) || self.class_memory.contains(&0) {
            return Err("palw_court_capacity_v1: every operation class must be measured (time, bytes and working set)");
        }
        let unit = *self.class_ns.iter().min().expect("four classes");
        let mut op_work = [0u64; PALW_COURT_OP_CLASSES_V1];
        let mut memory_per_byte = 1u64;
        for class in 0..PALW_COURT_OP_CLASSES_V1 {
            op_work[class] = self.class_ns[class].div_ceil(unit);
            memory_per_byte = memory_per_byte.max(self.class_memory[class].div_ceil(self.class_bytes[class]));
        }
        let policy = PalwCourtCapacityPolicyV1 {
            version: PALW_COURT_CAPACITY_POLICY_VERSION_V1,
            block_budget: PalwCourtCostV1 {
                work: self.block_court_ns / unit,
                bytes: self.block_court_bytes,
                memory: self.block_court_memory,
            },
            op_work,
            memory_per_byte,
            rho_sompi: self.rho_sompi,
            session_weight: 0,
            ticket_window_daa: self.ticket_window_daa,
            one_move_budget: PalwCourtCostV1 {
                work: self.block_one_move_ns / unit,
                bytes: self.block_one_move_bytes,
                memory: self.block_court_memory,
            },
            reserve_permille: self.reserve_permille,
        };
        policy.validate()?;
        let heaviest = (0..PALW_COURT_OP_CLASSES_V1).all(|class| {
            let cost = PalwCourtCostV1 {
                work: op_work[class],
                bytes: self.class_bytes[class],
                memory: self.class_bytes[class].saturating_mul(memory_per_byte),
            };
            let budget = if class == PalwCourtOpClassV1::OneMove as usize { policy.one_move_budget } else { policy.block_budget };
            cost.fits_within(&budget)
        });
        if !heaviest {
            return Err("palw_court_capacity_v1: the block allotments cannot take every operation class at its measured worst once");
        }
        Ok(policy)
    }
}

/// **The fence**: its height, its policy, and the measurement record the policy is derived from (`None` only for the placeholder and
/// on an isolated test genesis). The policy is what the fold reads and what the ids hash; the measurement is what validation checks
/// the policy against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwCourtCapacityFenceV1 {
    pub activation: ForkActivation,
    pub policy: PalwCourtCapacityPolicyV1,
    pub measurement: Option<PalwCourtMeasurementV1>,
}

impl PalwCourtCapacityFenceV1 {
    /// The fence at `activation` with the placeholder policy (the fork-id probe, the A-2 harness; refused when armed).
    pub fn placeholder_at(activation: ForkActivation) -> Self {
        Self { activation, policy: palw_court_capacity_placeholder_policy_v1(), measurement: None }
    }

    /// The fence at `activation` with the policy `measurement` derives.
    pub fn measured_at(activation: ForkActivation, measurement: PalwCourtMeasurementV1) -> Result<Self, &'static str> {
        Ok(Self { activation, policy: measurement.policy()?, measurement: Some(measurement) })
    }
}

/// **The fold's mirror** (`PalwStateParamsV2::court_capacity_v1`): the height and the policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwCourtCapacityMirrorV1 {
    pub from_daa: u64,
    pub policy: PalwCourtCapacityPolicyV1,
}

impl PalwCourtCapacityMirrorV1 {
    pub fn active_at(&self, daa: u64) -> bool {
        daa >= self.from_daa
    }
}

impl Params {
    /// `palw_court_capacity_v1`, resolved: `Some` only on a `ConsensusV2` network with a real height.
    pub fn palw_court_capacity_v1_fence(&self) -> Option<PalwCourtCapacityFenceV1> {
        match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(_) => self.palw_court_capacity_v1.filter(|f| f.activation != ForkActivation::never()),
            _ => None,
        }
    }

    /// The fence's mirror on the V2 bundle's state params, which the fold and the acceptance walk read. Written here and nothing else.
    pub fn sync_palw_court_capacity_v1(&mut self) {
        let mirror = self
            .palw_court_capacity_v1
            .filter(|f| f.activation != ForkActivation::never())
            .map(|f| PalwCourtCapacityMirrorV1 { from_daa: f.activation.daa_score(), policy: f.policy });
        if let PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_court_capacity_v1(mirror);
        }
    }

    /// **The fence's refusals** (`validate_palw_v2`): the mirror agrees; a `ConsensusV2` rule; `palw_legacy_public_filer_v1` in force at
    /// or below it (court admission needs the challenger's reservation: its claim-fixed deadline and Final hold); a structurally sound
    /// policy; on a real network the arm-readiness record (`PalwG14GatedFenceV1::CourtCapacityV1`), a non-placeholder policy, and a
    /// measurement record from which the policy derives exactly and whose digest is the record's measured-values evidence.
    pub fn validate_palw_court_capacity_in_v1(
        &self,
        records: &[crate::palw_g14_gates_v1::PalwG14ArmingRecordV1],
    ) -> Result<(), PalwModeV2Error> {
        let armed = self.palw_court_capacity_v1.filter(|f| f.activation != ForkActivation::never());
        if let PalwConsensusMode::ConsensusV2(bundle) = &self.palw_consensus_mode {
            let want = armed.map(|f| PalwCourtCapacityMirrorV1 { from_daa: f.activation.daa_score(), policy: f.policy });
            if bundle.state.court_capacity_v1() != want {
                return Err(PalwModeV2Error::Invalid(
                    "palw_court_capacity_v1 disagrees with the V2 bundle's mirror: Params::sync_palw_court_capacity_v1",
                ));
            }
        }
        let Some(fence) = armed else { return Ok(()) };
        if !matches!(self.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)) {
            return Err(PalwModeV2Error::Invalid("palw_court_capacity_v1 is a ConsensusV2 rule: this ruleset has no V2 bundle"));
        }
        let at = fence.activation.daa_score();
        if !self.palw_legacy_public_filer_v1.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= at) {
            return Err(PalwModeV2Error::Invalid(
                "palw_court_capacity_v1 needs palw_legacy_public_filer_v1 in force at or below it: court admission reads the \
                 challenger's own dispute reservation (its claim-fixed hard deadline and Final hold)",
            ));
        }
        fence.policy.validate().map_err(PalwModeV2Error::Invalid)?;
        if !self.has_isolated_g14_genesis_v1() && fence.policy.is_placeholder() {
            return Err(PalwModeV2Error::Invalid(
                "palw_court_capacity_v1 cannot be armed with the placeholder policy: the block budget, the operation work units and \
                 rho_court must be the measured values of the slowest supported node",
            ));
        }
        self.validate_palw_g14_fence_record_in_v1(crate::palw_g14_gates_v1::PalwG14GatedFenceV1::CourtCapacityV1, records)?;
        if let Some(measurement) = fence.measurement
            && measurement.policy() != Ok(fence.policy)
        {
            return Err(PalwModeV2Error::Invalid(
                "palw_court_capacity_v1's policy is not the one its measurement record derives (PalwCourtMeasurementV1::policy)",
            ));
        }
        if self.has_isolated_g14_genesis_v1() {
            return Ok(());
        }
        let Some(measurement) = fence.measurement else {
            return Err(PalwModeV2Error::Invalid(
                "palw_court_capacity_v1 needs its measurement record: the policy must derive from the slowest supported node's measured \
                 court operations (PalwCourtMeasurementV1)",
            ));
        };
        let record = records.iter().find(|record| {
            record.fence == crate::palw_g14_gates_v1::PalwG14GatedFenceV1::CourtCapacityV1
                && record.genesis == self.genesis.hash
                && record.activation_daa == at
        });
        if record.and_then(|record| record.evidence.get(PALW_COURT_MEASUREMENT_EVIDENCE_INDEX_V1)) != Some(&measurement.digest()) {
            return Err(PalwModeV2Error::Invalid(
                "palw_court_capacity_v1's arm-readiness record does not carry this measurement record's digest as its measured-values \
                 evidence",
            ));
        }
        Ok(())
    }
}

// =================================================================================================================================
// 3. The deadline-aware scheduler (pure)
// =================================================================================================================================

/// **One queued prosecution**, as the scheduler orders it: earliest claim hard deadline first, then the earliest queued, then the
/// claim and the reserver (a total order every node computes alike).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PalwCourtQueueKeyV1 {
    pub deadline_daa: u64,
    pub enqueued_daa: u64,
    pub claim: Hash64,
    pub reserver: PalwBondKeyV2,
}

/// A queued prosecution with the service the scheduler accounts for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwCourtQueuedV1 {
    pub key: PalwCourtQueueKeyV1,
    /// Its maximum session (the session window it would open with), DAA.
    pub service_daa: u64,
}

/// **Worst-case service accounting**: the capacity's slots are released at `occupant_releases` (each occupant held to its backstop;
/// slots never occupied are free `now`); every queued prosecution, in key order, takes the earliest released slot, holds it for the
/// ticket window plus its maximum session, and must be granted early enough that its session, opened at the end of its ticket and
/// held to its maximum, still ends by its claim's hard deadline (`grant + ticket_window + service ≤ deadline`: the verdict lands
/// inside the deadline). Returns each entry's worst-case grant DAA, or the index (in key order) of the first entry whose guarantee
/// fails.
pub fn palw_court_schedule_v1(
    now: u64,
    capacity: u64,
    occupant_releases: &[u64],
    queue: &[PalwCourtQueuedV1],
    ticket_window: u64,
) -> Result<Vec<u64>, usize> {
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;
    let mut slots: BinaryHeap<Reverse<u64>> = occupant_releases.iter().map(|r| Reverse((*r).max(now))).collect();
    let free = (capacity as usize).saturating_sub(occupant_releases.len());
    // Never materialize more free slots than the queue can use.
    for _ in 0..free.min(queue.len()) {
        slots.push(Reverse(now));
    }
    let mut ordered: Vec<(usize, &PalwCourtQueuedV1)> = queue.iter().enumerate().collect();
    ordered.sort_by_key(|(_, entry)| entry.key);
    let mut grants = vec![0u64; queue.len()];
    for (position, (index, entry)) in ordered.into_iter().enumerate() {
        let Some(Reverse(grant)) = slots.pop() else { return Err(position) };
        if grant.saturating_add(ticket_window).saturating_add(entry.service_daa) > entry.key.deadline_daa {
            return Err(position);
        }
        grants[index] = grant;
        slots.push(Reverse(grant.saturating_add(ticket_window).saturating_add(entry.service_daa)));
    }
    Ok(grants)
}

// =================================================================================================================================
// 4. The prosecution's row (inside its LG14-A reservation)
// =================================================================================================================================

/// **Where one reserved prosecution stands in the court-capacity scheduler** — kept beside its reservation in the claim's dispute
/// record (`crate::palw_legacy_public_filer_v1::PalwDisputeClaimV1::courts`, keyed by the reserver), so it rides LG14-A's journaled
/// table (delta 200, tail 0xE2, root block `legacy_disputes/v1`) and its restart / reorg / IBD paths unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwCourtStageV1 {
    /// Waiting for capacity, admitted under the worst-case guarantee.
    Queued { enqueued_daa: u64 },
    /// Capacity granted at `granted_daa`; the prosecutor files its accusation by `expires_daa` or loses the grant.
    Ticket { granted_daa: u64, expires_daa: u64 },
    /// The session `session` is open; occupancy is metered from `started_daa` (its grant, or its opening).
    Running { session: Hash64, started_daa: u64 },
}

/// **The prosecution's court-capacity row**: its stage, the occupancy escrow it holds on the reserver's free half (A-6), and the
/// price terms fixed at admission (so no later policy re-prices it).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwReservedCourtV1 {
    pub stage: PalwCourtStageV1,
    /// ρ_court × (ticket window + maximum session) × weight, held until settlement.
    pub escrow: u128,
    pub rho_sompi: u64,
    pub weight: u64,
    /// The maximum session (DAA) the scheduler accounted for after the ticket window (the class's court window at admission).
    pub service_daa: u64,
    /// **Round 2: the claim's reserve row** (its first court row, on the reserve slots) — else a spare row.
    pub reserve: bool,
    /// **Round 2: a spare row admitted under the worst-case deadline guarantee** (always `true` for a reserve row); `false` = admitted
    /// best-effort, granted only behind every guaranteed spare row.
    pub guaranteed: bool,
    /// **Round 2: the DAA the row was admitted at** (the unserved-claim rule reads it).
    pub admitted_daa: u64,
}

impl PalwReservedCourtV1 {
    /// The DAA occupancy is metered from, if the prosecution holds capacity.
    pub fn metered_from(&self) -> Option<u64> {
        match self.stage {
            PalwCourtStageV1::Queued { .. } => None,
            PalwCourtStageV1::Ticket { granted_daa, .. } => Some(granted_daa),
            PalwCourtStageV1::Running { started_daa, .. } => Some(started_daa),
        }
    }

    /// **Round 2: is the row still waiting for its court** (queued, or holding an unused ticket)?
    pub fn waiting(&self) -> bool {
        matches!(self.stage, PalwCourtStageV1::Queued { .. } | PalwCourtStageV1::Ticket { .. })
    }

    /// The occupancy fee owed at `now`: ρ × occupied DAA × weight, never more than the escrow.
    pub fn fee_at(&self, now: u64) -> u128 {
        let occupied = self.metered_from().map_or(0, |from| now.saturating_sub(from));
        u128::from(self.rho_sompi).saturating_mul(u128::from(occupied)).saturating_mul(u128::from(self.weight)).min(self.escrow)
    }
}

// =================================================================================================================================
// 5. The per-claim court / DA budget (the detection-policy lane's interface, g14-detection-policy §10)
// =================================================================================================================================

/// **What the court and the data-availability route reserve for ONE claim for its liability horizon — capacity no other bond can
/// fill** (`palw_court_claim_budget_v1`):
///
/// * `compellable_openings`: the units ONE honest reservation can compel on the claim on its own budget — its reserved DA sessions
///   ([`crate::palw_legacy_public_filer_v1::PALW_DISPUTE_SESSIONS_PER_RESERVATION_V1`]) and, where LG14-B is in force at the claim's
///   acceptance, its extra parallel lanes ([`crate::palw_legacy_parallel_v1::PALW_LEGACY_EXTRA_LANES_PER_RESERVATION_V1`]). Another
///   bond's reservation spends its OWN budget (one reservation per bond per claim), never this one.
/// * `da_bytes`: those units at the largest answer the chain carries (`PALW_OBJECT_CHUNK_MAX_COUNT × PALW_OBJECT_CHUNK_MAX_BYTES`).
/// * `first_move_by_daa`: the last DAA a prosecution may start and still be guaranteed its verdict — the claim's hard deadline
///   (acceptance + the class's promised trace retention, `palw_attempt_trace_retention_daa_v1`, − the disclosure window)
///   less the guarantee's lead: `4W` on the reserve pool for a court-demanding claim (`PalwCourtCapacityPolicyV1::reserve_lead_daa`),
///   one service window `W` otherwise.
/// * `reserved_sessions`: court sessions reserved to the claim — 1 for a court-demanding claim (its reserve row, guaranteed by the
///   bounded admission), 0 for a claim adjudicated in one move (the one-move budget is per block, not per claim).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwCourtClaimBudgetV1 {
    pub compellable_openings: u32,
    pub da_bytes: u64,
    pub first_move_by_daa: u64,
    pub reserved_sessions: u32,
}

/// **The per-claim budget of a claim of `class_id` accepted at `accepted_daa`** — `None` below `palw_court_capacity_v1` (nothing is
/// reserved per claim there). A pure read of state and params; the claim gate (`palw_detection_policy_v1`) checks a policy's declared
/// needs against it.
pub fn palw_court_claim_budget_v1(
    state: &PalwChainStateV2,
    params: &crate::palw_state_v2::PalwStateParamsV2,
    class_id: &Hash64,
    accepted_daa: u64,
) -> Option<PalwCourtClaimBudgetV1> {
    let retention = accepted_daa.saturating_add(palw_attempt_trace_retention_daa_v1(state, params, class_id, accepted_daa));
    palw_court_claim_budget_for_retention_v1(state, params, class_id, accepted_daa, retention)
}

/// [`palw_court_claim_budget_v1`] for a claim whose promised trace retention is known (`trace_retention_daa`, the attempt's or the
/// commitment's own field): the hard deadline is the claim's own (`retention − W_disclose`), not the producer minimum's lower bound.
pub fn palw_court_claim_budget_for_retention_v1(
    state: &PalwChainStateV2,
    params: &crate::palw_state_v2::PalwStateParamsV2,
    class_id: &Hash64,
    accepted_daa: u64,
    trace_retention_daa: u64,
) -> Option<PalwCourtClaimBudgetV1> {
    let mirror = params.court_capacity_v1().filter(|m| m.active_at(accepted_daa))?;
    let policy = mirror.policy;
    let fused = state.class(class_id).is_some_and(|class| class.fused_attention);
    let held_da = params.g14_route_required_v1().and_then(|m| m.legacy_held_da_from).is_some_and(|from| accepted_daa >= from);
    let mut compellable = u32::from(crate::palw_legacy_public_filer_v1::PALW_DISPUTE_SESSIONS_PER_RESERVATION_V1);
    if held_da {
        compellable = compellable.saturating_add(u32::from(crate::palw_legacy_parallel_v1::PALW_LEGACY_EXTRA_LANES_PER_RESERVATION_V1));
    }
    let unit_bytes =
        (crate::palw_state_v2::PALW_OBJECT_CHUNK_MAX_COUNT as u64).saturating_mul(crate::palw_state_v2::PALW_OBJECT_CHUNK_MAX_BYTES as u64);
    let turn = params.turn_deadline_daa();
    let hard = trace_retention_daa.saturating_sub(crate::palw_state_v2::palw_da_disclose_window_daa_v1(params));
    let lead = if fused {
        policy.reserve_lead_daa(turn, params.window_court())
    } else {
        policy.admission_bucket_daa(turn, params.window_court())
    };
    Some(PalwCourtClaimBudgetV1 {
        compellable_openings: compellable,
        da_bytes: u64::from(compellable).saturating_mul(unit_bytes),
        first_move_by_daa: hard.saturating_sub(lead),
        reserved_sessions: u32::from(fused),
    })
}

// =================================================================================================================================
// 6. The court-demanding claim's retention (user decision 2026-10-17)
// =================================================================================================================================

/// **The trace retention a court-demanding claim promises past `palw_court_capacity_v1`** (user decision 2026-10-17), from its
/// acceptance: at least `4W + W_disclose` (the reserve guarantee's lead plus the disclosure window) — and counted from the claim's
/// licence (`window_bind + window_receipt` after acceptance), so the last guaranteed first move `hard − 4W` falls no earlier than the
/// licence, when a seat or an outside verifier can first know the claim is worth prosecuting. Never below the producer minimum
/// (`palw_min_trace_retention_daa_v1`). `None` below the fence (the minimum alone).
pub fn palw_court_demanding_retention_daa_v1(params: &crate::palw_state_v2::PalwStateParamsV2, daa: u64) -> Option<u64> {
    let policy = params.court_capacity_at_v1(daa)?;
    let lead = policy.reserve_lead_daa(params.turn_deadline_daa(), params.window_court());
    let need = lead
        .saturating_add(crate::palw_state_v2::palw_da_disclose_window_daa_v1(params))
        .saturating_add(params.window_bind())
        .saturating_add(params.window_receipt());
    Some(need.max(crate::palw_producer_v2::palw_min_trace_retention_daa_v1(params)))
}

/// **The retention a V2 Attempt of `class_id` must promise at `daa`** (the producer's derivation and the fold's per-class pin): the
/// court-demanding retention for a fused-attention class past the fence, the producer minimum otherwise.
pub fn palw_attempt_trace_retention_daa_v1(
    state: &PalwChainStateV2,
    params: &crate::palw_state_v2::PalwStateParamsV2,
    class_id: &Hash64,
    daa: u64,
) -> u64 {
    let fused = state.class(class_id).is_some_and(|class| class.fused_attention);
    // RFC-0008 paid consumer lifecycle (validation lane): the minimum is the canonical retention at this DAA, which past its fence
    // covers the consumer's service, dispute and material tails; a court-demanding class owes the longer of the two.
    let minimum = crate::palw_producer_v2::palw_min_trace_retention_daa_at_v2(params, daa);
    let base = match palw_court_demanding_retention_daa_v1(params, daa) {
        Some(court) if fused => court.max(minimum),
        _ => minimum,
    };
    // RFC-0018 Gate C (user decision 2026-10-17): and through the latest certification and receipt lane (0 below its fence).
    base.saturating_add(crate::palw_rfc0018_v1::palw_gate_c_retention_extension_daa_v1(params, daa))
}

