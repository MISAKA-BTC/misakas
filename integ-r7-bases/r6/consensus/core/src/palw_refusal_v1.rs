//! **A refusal a registrant can read before paying the fee, in one shape for every tool** (2026-10-03: a Qwen3.5-9B registration was
//! turned away by a tool that printed only "needs 5,102 DAA, window 3,000" — no rule name, no fence, no way to see that the figure belonged
//! to a clock the network does not charge). [`PalwRefusalV1`] carries the code, the rule it names, the numbers (needed against limit, with
//! their unit) and what decided the reading — the fence and its height, or the ruleset — and serialises to the JSON every tool prints
//! beside its sentence (`misaka model preflight --json`, the SDK's pre-signing gate, the node's preflight verdict).
//!
//! It is read off the one error the gate raises ([`PalwClassAdmissionError`]); nothing here decides anything.

use crate::palw_class_admission_v2::PalwClassAdmissionError;
use serde::Serialize;

/// The refusal, structured.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PalwRefusalV1 {
    /// The gate's stable code (`COURT_WINDOW_TOO_SHORT`, `DEEPER_THAN_THE_LADDER`, …).
    pub code: String,
    /// The rule, in words: the inequality or condition the code names.
    pub rule: String,
    /// What the class needs, and the limit the ruleset gives it, where the rule is a number against a number.
    pub needed: Option<u64>,
    pub limit: Option<u64>,
    pub unit: Option<String>,
    /// What decided the reading the numbers come from: the regime (`held clock` / `ladder clock`), the fence that selects it and the height
    /// the refusal was read at.
    pub decided_by: String,
    /// The gate's own sentence.
    pub message: String,
}

impl PalwRefusalV1 {
    /// The JSON every tool prints beside its sentence.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// What decided the court-window reading at `daa`: the network's regime, as the gate read it (`PalwHeldAdmissionV1::armed` of its shape).
pub fn palw_refusal_decided_by_v1(held_armed: bool, daa: Option<u64>, held_context_from: Option<u64>) -> String {
    let at = daa.map_or_else(|| "the height of the shape it was asked under".to_string(), |d| format!("DAA {d}"));
    if held_armed {
        match held_context_from {
            Some(from) => format!("the held clock (no leaf ladder): palw_held_context, armed from DAA {from}, read at {at}"),
            None => format!("the held clock (no leaf ladder): palw_held_context armed, read at {at}"),
        }
    } else {
        format!("the ladder clock: palw_held_context is not armed, read at {at}")
    }
}

impl PalwClassAdmissionError {
    /// The refusal as a structured value; `decided_by` names the regime, fence and height it was read under
    /// ([`palw_refusal_decided_by_v1`]).
    pub fn refusal_v1(&self, decided_by: &str) -> PalwRefusalV1 {
        let (rule, needed, limit, unit): (String, Option<u64>, Option<u64>, Option<&str>) = match self {
            Self::CourtWindowTooShort { needed, window } => (
                "the whole dispute (history rounds, and the leaf ladder only under the ladder clock) plus the assembly reserve fits window_court".into(),
                Some(*needed),
                Some(*window),
                Some("DAA"),
            ),
            Self::DeeperThanTheLadder { worst, ladder } => {
                ("the class's worst-case step leaves fit the ruleset's ladder".into(), Some(*worst), Some(*ladder), Some("step leaves"))
            }
            Self::CourtCostExceedsCeiling { what, got, ceiling } => {
                (format!("the court's {what} stays within its ceiling"), Some(*got), Some(*ceiling), None)
            }
            Self::CanonicalDeeperThanWorstCase { canonical, worst } => {
                ("the canonical job is no deeper than the class's worst case".into(), Some(*canonical), Some(*worst), Some("step leaves"))
            }
            Self::CanonicalFootprintUnderTheRow { footprint, floor } => (
                format!("the canonical job touches at least the row's floor of cached positions (this one touches {footprint})"),
                Some(*floor),
                None,
                Some("cached positions"),
            ),
            Self::TirExceeds { limit, at, value, cap } => (format!("the program's {limit} at {at} stays within the ceiling"), Some(*value), Some(*cap), None),
            other => (other.to_string(), None, None, None),
        };
        PalwRefusalV1 {
            code: self.code().to_string(),
            rule,
            needed,
            limit,
            unit: unit.map(str::to_string),
            decided_by: decided_by.to_string(),
            message: self.to_string(),
        }
    }
}
