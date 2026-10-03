//! **The Hugging Face census** (RFC-0002 §II.10, R6/A6): how many public Hub model repositories — counted as `repo_id@revision`, not
//! as architecture examples — pass each gate from source to `Final`, and what stops the rest.
//!
//! * [`listing`] — one repository as the Hub's listing knows it: the declared task, the one artifact it is judged by (a published
//!   selection rule), its strata, and the plan of what a header fetch reads.
//! * [`tasks`] — the declared task and the canonical job profile it needs (a versioned table).
//! * [`store`] — the header store a fetch wrote (`tools/hf_census/fetch.py`), and the preflight source built from it.
//! * [`gates`] — the row: six gates, one stable `blocking` code per failed gate, `NOT_RUN_AFTER_<GATE>` after a failure, a strict view
//!   with the rights policy applied and a technical view without it.
//! * [`codes`] — the gates' codes and the one table from the preflight's codes (§II.2.4) to them.
//! * [`rights`] — the rights policies (`none`, the headline's; a proposal for the lead to decide).
//!
//! The HTTP side (enumeration, fetching headers by exact byte ranges, sampling and the statistics) is `tools/hf_census/`. Nothing in
//! this module opens a connection.

pub mod cli;
pub mod codes;
pub mod gates;
pub mod listing;
pub mod rights;
pub mod store;
pub mod tasks;

pub use codes::{Gate, GateStatus};
pub use gates::{CensusContext, CensusRowV1, ROW_SCHEMA_V1, evaluate};
pub use listing::{ListingV1, PlanV1, plan_of, select, strata_of, task_of};
pub use rights::RightsPolicy;

/// What `palw-class census classify` prints per repository: its row at the listing depth and the plan a header fetch would execute.
#[derive(Clone, Debug, serde::Serialize)]
pub struct ClassifiedV1 {
    pub row: CensusRowV1,
    pub plan: PlanV1,
    /// The listing decided the technical outcome (a failure the headers cannot change), so the repository need not be fetched.
    pub decided: bool,
}

/// The listing-depth row and the fetch plan of one repository.
pub fn classify(l: &ListingV1, ctx: &CensusContext) -> ClassifiedV1 {
    let row = evaluate(l, None, ctx);
    let plan = plan_of(&row.selected);
    let decided = row.technical.iter().any(|g| g.status == GateStatus::Fail);
    ClassifiedV1 { row, plan, decided }
}
