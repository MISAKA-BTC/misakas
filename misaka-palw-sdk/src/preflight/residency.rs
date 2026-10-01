//! **What a class holds in memory, and what one replay reads from storage** (RFC-0002 Part II §II.2; ADR-0112
//! for IR classes, `docs/design/palw/tir/runtime-residency.md`) — from the program alone, through the tier function a
//! node holds the class by (`misaka_palw_tir_exec::tiers`), so a model is sized before a byte of it is converted:
//!
//! * the weights on disk, split into the three tiers the program's dataflow decides — **pinned** (read whole by every
//!   forward, held), **routed** (rows a route selects: a mixture's experts, held under what the budget leaves) and
//!   **gathered** (rows an input selects: embeddings, n-gram tables, read a row at a time and never held whole);
//! * the **floor** — the pinned set, one token's routed rows and one admission in flight — and the **default budget**,
//!   a fifth of the weights, and whether the default holds the floor (if not, a node holds the class through the page
//!   cache unless its operator states a budget at least the floor);
//! * the bytes **one replay** reads at the class's canonical job — the expected union of the routed rows its forward
//!   passes choose, and its gathered rows — and how long that is at the reference read rates. ESTIMATES: the union
//!   assumes independent uniform routing; the rates are ADR-0112 §1's (845 MB/s, a fleet host's direct read; about
//!   500 MB/s with a producer running), not this host's.
//!
//! A seat's resources gate its readiness, never a class's admission: nothing here enters a verdict.

use misaka_palw_tir::TirProgramV1;
use misaka_palw_tir_exec::tiers::{TirTierRulesV1, TirTierV1, TirTiersV1};
use serde::Serialize;

/// The reference read rates a replay's read time is estimated at, in MB/s (ADR-0112 §1).
pub const PREFLIGHT_READ_RATES_MB_S_V1: [u64; 2] = [500, 845];

/// One routed or gathered param.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ResidencyParamInfo {
    pub name: String,
    pub tier: &'static str,
    pub instances: u32,
    pub instance_bytes: u64,
    /// The gathered view's rows, the bytes of one, and the most rows one forward reads of one instance.
    pub rows: u32,
    pub row_bytes: u64,
    pub rows_per_forward: u64,
}

/// What one replay reads from storage.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ReplayReadInfo {
    /// The canonical job `(prefill, decode)` the estimate is at, when the depth reached chose a context; `None`: one
    /// forward pass.
    pub job: Option<(u32, u32)>,
    pub forwards: u64,
    /// One forward's routed rows and gathered rows.
    pub one_forward_bytes: u64,
    /// The routed rows the job's forwards are expected to choose, each read once (a cold cache).
    pub routed_union_bytes: u64,
    /// The gathered rows the job reads (never held: read every forward).
    pub gathered_bytes: u64,
    /// The two together: what a seat already holding the class reads for one replay.
    pub bytes: u64,
    /// Seconds at each of [`PREFLIGHT_READ_RATES_MB_S_V1`] — estimates.
    pub seconds_at: Vec<(u64, f64)>,
}

/// **The class's residency**, as a node would hold it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ResidencyInfo {
    /// The rule a row-addressed param under this many bytes is pinned by (the node's default).
    pub pin_below_bytes: u64,
    /// The program's params, every instance: the weights on disk.
    pub weight_bytes: u64,
    pub pinned_bytes: u64,
    pub routed_bytes: u64,
    /// One token's routed rows.
    pub routed_token_bytes: u64,
    pub gathered_bytes: u64,
    pub gathered_token_bytes: u64,
    /// The largest single admission — one route group's rows — held while a gather copies them.
    pub in_flight_bytes: u64,
    /// The least budget a node holds the class in (a stated budget below it is refused).
    pub floor_bytes: u64,
    /// A fifth of the weights: the default budget where the host can spare it.
    pub default_budget_bytes: u64,
    /// The default holds the floor (else a node holds the class through the page cache unless a budget is stated).
    pub default_holds_floor: bool,
    pub replay: ReplayReadInfo,
    /// The routed and gathered params (every other param is pinned).
    pub rows: Vec<ResidencyParamInfo>,
    pub note: String,
}

/// **The residency of `program`** under `rules`, with one replay at `canonical` (`(prefill, decode)`; `None`: one
/// forward pass).
pub fn residency_of(program: &TirProgramV1, rules: TirTierRulesV1, canonical: Option<(u32, u32)>) -> ResidencyInfo {
    let tiers = TirTiersV1::of(program, rules);
    let a = tiers.arithmetic();
    let forwards = canonical.map_or(1, |(p, d)| (u64::from(p) + u64::from(d)).saturating_sub(1).max(1));
    let routed_union_bytes = tiers.routed_union_bytes(forwards);
    let gathered = a.gathered_token_bytes.saturating_mul(forwards);
    let bytes = routed_union_bytes.saturating_add(gathered);
    let seconds_at = PREFLIGHT_READ_RATES_MB_S_V1.iter().map(|mb| (*mb, bytes as f64 / (*mb as f64 * 1e6))).collect();
    let rows = tiers
        .params
        .iter()
        .enumerate()
        .filter(|(_, t)| t.tier.is_rows())
        .map(|(j, t)| ResidencyParamInfo {
            name: program.params[j].name.clone(),
            tier: t.tier.name(),
            instances: t.instances.len() as u32,
            instance_bytes: t.instance_bytes,
            rows: t.rows,
            row_bytes: t.unit_bytes(program.params[j].dtype.width()),
            rows_per_forward: t.per_forward,
        })
        .collect::<Vec<_>>();
    let routed = tiers.params.iter().filter(|t| t.tier == TirTierV1::Routed).count();
    ResidencyInfo {
        pin_below_bytes: rules.pin_below_bytes,
        weight_bytes: a.weight_bytes,
        pinned_bytes: a.pinned_bytes,
        routed_bytes: a.routed_bytes,
        routed_token_bytes: a.routed_token_bytes,
        gathered_bytes: a.gathered_bytes,
        gathered_token_bytes: a.gathered_token_bytes,
        in_flight_bytes: a.in_flight_bytes,
        floor_bytes: a.floor_bytes,
        default_budget_bytes: a.fifth_bytes,
        default_holds_floor: a.fifth_bytes >= a.floor_bytes,
        replay: ReplayReadInfo {
            job: canonical,
            forwards,
            one_forward_bytes: a.routed_token_bytes.saturating_add(a.gathered_token_bytes),
            routed_union_bytes,
            gathered_bytes: gathered,
            bytes,
            seconds_at,
        },
        rows,
        note: if routed == 0 {
            "nothing is routed: a node holds the pinned set and reads the gathered rows; its floor is close to the weights, \
             so the default fifth leaves the class on the page cache unless a budget is stated"
                .into()
        } else {
            "the routed rows a replay reads are the expected union of independent uniform routes; the read times are \
             estimates at reference rates (ADR-0112 §1), not this host's"
                .into()
        },
    }
}
