//! **The mirror check** (RFC-0007 Part II, "Activation"): the seat's algebraic checker run **beside the full replay on every claim the seat
//! already replays**, so that before any seat relies on the checker its verdict is compared, claim by claim, with the replay's — and any
//! disagreement is logged and alarmed.
//!
//! Two cases, one function ([`tir_mirror_agreement_v1`]):
//!
//! * **own witness** — the seat has no served witness for the claim, so it produces one itself on the typed backend (honest by
//!   construction) and checks it. The checker must accept it: a rejection is a **false reject**, a defect in the checker or its sketches.
//! * **a served witness** — the producer's. If the replay reproduces the claim, a correct witness is accepted (a refusal says only that
//!   *this witness* is wrong, which is no disagreement). If the replay does **not** reproduce the claim, the checker must not accept a
//!   witness whose derived rows are the claim's: an acceptance is a **false accept**, the alarm that matters (a soundness failure, or the
//!   `1/p` the proof allows).
//!
//! The function that runs the check ([`tir_mirror_check_v1`]) is pure over its inputs; the node holds the stores, the secret and the
//! counters.

use misaka_palw_tir::{ParamSource, TirResult};
use misaka_palw_tir_exec::TirPlan;

use crate::analysis::{TirCheckPolicyV1, TirSketchAnalysisV1};
use crate::check::{TirCheckFailureV1, TirCheckReportV1, TirSketchCheckerV1};
use crate::secret::TirSketchKeysV1;
use crate::sketch::TirSketchStoreV1;
use crate::witness::{TirSketchJobV1, TirWitnessV1};

/// What one mirror check found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TirMirrorOutcomeV1 {
    pub accepted: bool,
    pub report: Option<TirCheckReportV1>,
    pub failure: Option<TirCheckFailureV1>,
    /// The bytes the witness serves at their declared width (what the check read over the link, in a real deployment).
    pub served_bytes: u64,
}

/// **Run the checker on `witness`** as the execution of `job`. `params` is what the seat holds (for the mirror, everything).
#[allow(clippy::too_many_arguments)]
pub fn tir_mirror_check_v1(
    plan: &TirPlan,
    analysis: &TirSketchAnalysisV1,
    store: &TirSketchStoreV1,
    keys: &TirSketchKeysV1,
    params: &dyn ParamSource,
    job: &TirSketchJobV1,
    job_id: &[u8; 32],
    witness: &TirWitnessV1,
    policy: TirCheckPolicyV1,
) -> TirResult<TirMirrorOutcomeV1> {
    let served_bytes = witness.served().1;
    let checker = TirSketchCheckerV1::new(plan, analysis, store, keys, params, policy)?;
    Ok(match checker.check(job, job_id, witness) {
        Ok(report) => TirMirrorOutcomeV1 { accepted: true, report: Some(report), failure: None, served_bytes },
        Err(failure) => TirMirrorOutcomeV1 { accepted: false, report: None, failure: Some(*failure), served_bytes },
    })
}

/// How the checker's verdict compares with the replay's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TirMirrorAgreementV1 {
    /// The two agree.
    Agree,
    /// The seat's own honest witness was refused: a defect in the checker or its sketches. **Alarm.**
    FalseReject,
    /// The checker accepted a witness of a claim the replay does not reproduce: a soundness failure (or the `1/p`). **Alarm.**
    FalseAccept,
    /// A served witness of a claim the replay reproduces was refused: the witness is wrong, which is the checker doing its job. Logged,
    /// not alarmed (the seat then falls back to the replay or escalates, §II.8).
    WitnessRefused,
}

impl TirMirrorAgreementV1 {
    /// Whether this is a disagreement an operator must be told of.
    pub fn is_alarm(self) -> bool {
        matches!(self, Self::FalseReject | Self::FalseAccept)
    }
}

/// **The comparison** (module note). `served_witness` is `true` when the witness came from the producer, `false` when the seat made it.
pub fn tir_mirror_agreement_v1(served_witness: bool, replay_reproduces_claim: bool, checker_accepted: bool) -> TirMirrorAgreementV1 {
    match (served_witness, replay_reproduces_claim, checker_accepted) {
        (false, _, true) => TirMirrorAgreementV1::Agree,
        (false, _, false) => TirMirrorAgreementV1::FalseReject,
        (true, true, true) => TirMirrorAgreementV1::Agree,
        (true, true, false) => TirMirrorAgreementV1::WitnessRefused,
        (true, false, false) => TirMirrorAgreementV1::Agree,
        (true, false, true) => TirMirrorAgreementV1::FalseAccept,
    }
}

/// **The mirror's counters** (the node's status line): every claim checked, the agreements, and each kind of disagreement.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TirMirrorTotalsV1 {
    pub checked: u64,
    pub agreed: u64,
    pub witness_refused: u64,
    pub false_rejects: u64,
    pub false_accepts: u64,
    pub served_bytes: u64,
    pub errors: u64,
}

impl TirMirrorTotalsV1 {
    pub fn note(&mut self, agreement: TirMirrorAgreementV1, served_bytes: u64) {
        self.checked += 1;
        self.served_bytes += served_bytes;
        match agreement {
            TirMirrorAgreementV1::Agree => self.agreed += 1,
            TirMirrorAgreementV1::WitnessRefused => self.witness_refused += 1,
            TirMirrorAgreementV1::FalseReject => self.false_rejects += 1,
            TirMirrorAgreementV1::FalseAccept => self.false_accepts += 1,
        }
    }

    /// The `key=value` pairs the status line carries.
    pub fn status(&self) -> String {
        format!(
            "sketch_mirror_checked={} sketch_mirror_agreed={} sketch_mirror_witness_refused={} sketch_mirror_false_rejects={} \
             sketch_mirror_false_accepts={} sketch_mirror_errors={} sketch_mirror_served_bytes={}",
            self.checked, self.agreed, self.witness_refused, self.false_rejects, self.false_accepts, self.errors, self.served_bytes
        )
    }
}
