//! RFC-0006 all-model boundary report: for EVERY class registered on the caller's authenticated chain (single-program,
//! pipeline and typed memory/retrieval/composite), the deterministic bounds the class carries from registration
//! (`ProsecutionBoundsV1`: public bytes a fresh verifier fetches, largest opening / filing / response, commit carrier,
//! localization rounds, worst single court work, verifier RAM, retained state, concurrent sessions, deadline) — i.e. the
//! worst cell's material, adjudication and acquisition size — side by side with the quantities that need MEASUREMENT
//! (wall time per court, fetch time, cost per byte/work) which stay UNKNOWN (`None`) here.
//!
//! A class the court cannot adjudicate is refused at registration and therefore never appears; refusing an unsupported
//! architecture is not "all models supported" (the RFC-0011 census keeps unsupported models in its denominator).
//! Reporting only: no consensus rule reads this.
use misaka_palw_kernel::{gate::ProsecutionBoundsV1, hash::Digest, ledger::KernelLedgerV1};

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelBoundsRowV1 {
    pub class: String,
    pub kind: &'static str,
    /// Decimal strings: several bounds are u128.
    pub max_public_bytes: String,
    pub max_opening_bytes: u64,
    pub max_filing_bytes: u64,
    pub max_response_bytes: String,
    pub max_commit_bytes: String,
    pub max_localization_rounds: u32,
    pub max_court_work: u64,
    pub max_verifier_ram: String,
    pub max_retained_state: String,
    pub max_concurrent_sessions: u32,
    pub deadline_daa: u64,
    /// Measured on reference hardware, per class: UNKNOWN until the MEAS lane supplies it.
    pub court_wall_time_ms: Option<u64>,
    pub fresh_fetch_time_ms: Option<u64>,
    pub court_cost_sompi: Option<u64>,
    pub material_hold_cost_per_daa_sompi: Option<u64>,
}

fn row(class: &Digest, kind: &'static str, b: &ProsecutionBoundsV1) -> ModelBoundsRowV1 {
    ModelBoundsRowV1 {
        class: class.iter().map(|b| format!("{b:02x}")).collect(),
        kind,
        max_public_bytes: b.max_public_bytes.to_string(),
        max_opening_bytes: b.max_opening_bytes,
        max_filing_bytes: b.max_filing_bytes,
        max_response_bytes: b.max_response_bytes.to_string(),
        max_commit_bytes: b.max_commit_bytes.to_string(),
        max_localization_rounds: b.max_localization_rounds,
        max_court_work: b.max_court_work,
        max_verifier_ram: b.max_verifier_ram.to_string(),
        max_retained_state: b.max_retained_state.to_string(),
        max_concurrent_sessions: b.max_concurrent_sessions,
        deadline_daa: b.deadline_daa,
        court_wall_time_ms: None,
        fresh_fetch_time_ms: None,
        court_cost_sompi: None,
        material_hold_cost_per_daa_sompi: None,
    }
}

/// Every registered class's derived bounds, in canonical class-id order per kind.
pub fn registered_model_bounds_v1(ledger: &KernelLedgerV1) -> Vec<ModelBoundsRowV1> {
    let mut rows = Vec::with_capacity(ledger.classes.len() + ledger.pipeline_classes.len() + ledger.typed.classes.len());
    rows.extend(ledger.classes.iter().map(|(id, c)| row(id, "program", &c.bounds)));
    rows.extend(ledger.pipeline_classes.iter().map(|(id, c)| row(id, "pipeline", &c.bounds)));
    rows.extend(ledger.typed.classes.iter().map(|(id, c)| row(id, "typed", &c.bounds)));
    rows
}
