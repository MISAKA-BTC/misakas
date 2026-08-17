//! ADR-0037 Decisions 2/9 + ADR-0038 Decisions C/D/G — the bonded dispute, its settlement
//! algebra, and the on-chain execution class state, pure.
//!
//! Three pieces, each arithmetic over the caller's facts (no store handle, no clock, no
//! registry) in the [`crate::palw_job_state`] mold:
//!
//! * [`PalwDisputeStateV3`] — the bonded dispute riding one job's `Disputed`/`Adjudicating`
//!   stretch. The job spine (ADR-0037 Decision 2) already carries the status; this object
//!   carries the *money and clock* of the dispute: whose bond backed the alarm, which
//!   refutation evidence opened it, and the dual deadline the court must conclude within
//!   (DAA ∧ MTP, ADR-0037 Decision 4 — one saturated mergeset cannot evaporate the window).
//! * [`settle_dispute_v3`] — the PURE settlement map from a court terminal to what happens
//!   to executor bond, challenger bond, job credit and class status. It encodes I9 (only
//!   `Convicted` slashes the executor) and I10 (`Unadjudicable` slashes NOBODY and freezes
//!   the class) as a total three-row table; every non-court status refuses. Settlements
//!   agree with the spine's own predicates by construction and by test — one table, no
//!   second opinion for a consumer to diverge on.
//! * [`PalwExecutionClassStateV3`] — ADR-0037 Decision 9's pruning-surviving class registry
//!   entry (compile-time fork parameters are how the audit's flag-day bugs happened), with
//!   the closed status machine `Inactive → Probation → Active → Frozen → Deprecated`, the
//!   A4 activation gate (a class only activates over a [`crate::palw_catalog_coverage`]
//!   certificate naming IT — ADR-0038 Decision C's court is only truth over a complete
//!   catalog), and the six-path gate [`PalwExecutionClassStateV3::check_path`]: only an
//!   `Active` class passes ANY consensus path. Freeze halts credit, not the chain — and a
//!   freeze deliberately does NOT appear as a path here for dispute continuation, because
//!   a freeze never stops a running dispute (Decision 9 verbatim: "existing disputes
//!   continue"). Under ADR-0038 Decision D the same freeze also removes the class from the
//!   difficulty domain set; that removal is the DAA consumer's read of this status, not a
//!   second flag.
//!
//! Consensus-inert: nothing constructs these types on any shipped network. The Track-C/D
//! change sets (ADR-0037/0038 §Implementation order) wire them; per those ADRs, partial
//! activation is forbidden — landing the algebra early is how the change set is built,
//! activating a subset of it is how fail-opens are preserved.

use crate::palw_catalog_coverage::PalwCatalogCoverageCertificateV1;
use crate::palw_job_state::{PalwDualDeadlineV3, PalwJobStatusV3};
use crate::tx::TransactionOutpoint;
use kaspa_hashes::Hash64;

// ---------------------------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------------------------

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwDisputeError {
    /// Settlement is defined ONLY over the three court terminals; asking for the settlement
    /// of any other status is a consumer bug, never a default answer (I7's spirit).
    #[error("status {status:?} is not a court terminal — settlement is defined only over Convicted/NoFaultFound/Unadjudicable")]
    NotACourtTerminal { status: PalwJobStatusV3 },
    /// The six-path gate refused: only an `Active` class passes any consensus path.
    #[error("execution class is {status:?}, not Active — path {path:?} refuses (ADR-0037 Decision 9)")]
    ClassNotActiveForPath { status: PalwClassStatusV3, path: PalwClassPathV1 },
    /// The class status machine is closed; an unlisted edge is a refusal, not a no-op.
    #[error("class transition {from:?} → {to:?} is not an edge of the closed status machine")]
    IllegalClassTransition { from: PalwClassStatusV3, to: PalwClassStatusV3 },
    /// A freeze without a recorded reason is an unauditable freeze — Decision 9 carries
    /// `freeze_reason` in chain state precisely so re-audit knows what it is re-auditing.
    #[error("freezing a class requires a recorded freeze reason")]
    FreezeNeedsReason,
    /// ADR-0038 A4: activation without a catalog-coverage certificate is how a class ships
    /// an `Unadjudicable` hole for a forger to farm.
    #[error("class {class_id} may not activate without a catalog-coverage certificate (ADR-0038 A4)")]
    ActivationNeedsCoverage { class_id: Hash64 },
    /// A certificate is only proof for the class it names — someone else's coverage is not ours.
    #[error("coverage certificate names class {certificate_class_id}, but activation targets class {class_id}")]
    CoverageCertificateClassMismatch { class_id: Hash64, certificate_class_id: Hash64 },
}

// ---------------------------------------------------------------------------------------------
// The bonded dispute
// ---------------------------------------------------------------------------------------------

/// The bonded dispute riding one job's `Disputed`/`Adjudicating` stretch. Opened when the
/// spine accepts `RefutationFiled` (I8: the refutation locks the job, it never deletes);
/// resolved by feeding the job's court terminal to [`settle_dispute_v3`]. Payees resolve
/// ONLY through the recorded bond outpoint (I4), same as the spine.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwDisputeStateV3 {
    /// The job this dispute locks — [`crate::palw_job_state::PalwJobStateV3::job_id`].
    pub job_id: Hash64,
    /// The challenger's bond — the ONLY source of the refund/forfeit destination (I4).
    pub challenger_bond_outpoint: TransactionOutpoint,
    /// The accepted refutation evidence (the court's entry point, ADR-0030–0033).
    pub refutation_evidence_id: Hash64,
    /// The DAA score at which the refutation was accepted on-chain.
    pub filed_daa: u64,
    /// Dual deadline for the adjudication to conclude (DAA ∧ MTP — one saturated mergeset
    /// cannot evaporate the court's window).
    pub adjudication_deadline: PalwDualDeadlineV3,
    /// The bonded amount at stake behind the alarm (sompi).
    pub challenger_bond_amount: u64,
}

// ---------------------------------------------------------------------------------------------
// Settlement — the pure map encoding I9/I10
// ---------------------------------------------------------------------------------------------

/// What one court terminal means for money and for the class. A PURE map — the only three
/// inputs that may destroy value are the three court terminals (I9), and `Unadjudicable`
/// destroys none (I10).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwDisputeSettlementV3 {
    /// Slash the executor's bond? True for exactly one terminal: `Convicted` (I9).
    pub slash_executor: bool,
    /// What happens to the challenger's bond.
    pub challenger_bond: PalwChallengerBondDispositionV3,
    /// May the job's inflation credit still mint? Mirrors the spine's `creditable()`.
    pub job_creditable: bool,
    /// Must the execution class auto-freeze? Mirrors the spine's `demands_class_freeze()`.
    pub freeze_class: bool,
}

/// The two things a settled dispute can do with the challenger's bond.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwChallengerBondDispositionV3 {
    /// The bond returns to its outpoint's owner untouched.
    Refunded,
    /// The bond is forfeited — the bonded false alarm pays (ADR-0027's spam price).
    Forfeited,
}

/// The settlement table (ADR-0037 Decision 2 / ADR-0038 Decision C):
///
/// * `Convicted` — slash the executor; challenger `Refunded` (the alarm was true; bounty
///   policy is the mint layer's, not encoded here); job not creditable; no class freeze.
/// * `NoFaultFound` — no slash; challenger `Forfeited` (a bonded false alarm pays —
///   ADR-0027's permissionless refutation stays spam-priced); job IS creditable (the
///   executor was right all along); no freeze.
/// * `Unadjudicable` — NOBODY slashed, challenger `Refunded` (I10 — the class's catalog
///   gap is not the challenger's fault); job NOT creditable; `freeze_class` is `true`.
///
/// Any other status refuses with [`PalwDisputeError::NotACourtTerminal`] — including the
/// non-court terminals `FinalizedAccepted`/`FinalizedRejected`, which never had a dispute
/// to settle.
pub fn settle_dispute_v3(terminal: PalwJobStatusV3) -> Result<PalwDisputeSettlementV3, PalwDisputeError> {
    use PalwChallengerBondDispositionV3 as B;
    use PalwJobStatusV3 as S;
    Ok(match terminal {
        // I9: the one and only executor slash.
        S::Convicted => {
            PalwDisputeSettlementV3 { slash_executor: true, challenger_bond: B::Refunded, job_creditable: false, freeze_class: false }
        }
        S::NoFaultFound => {
            PalwDisputeSettlementV3 { slash_executor: false, challenger_bond: B::Forfeited, job_creditable: true, freeze_class: false }
        }
        // I10: nobody slashed, nothing credited, the class freezes.
        S::Unadjudicable => {
            PalwDisputeSettlementV3 { slash_executor: false, challenger_bond: B::Refunded, job_creditable: false, freeze_class: true }
        }
        other => return Err(PalwDisputeError::NotACourtTerminal { status: other }),
    })
}

// ---------------------------------------------------------------------------------------------
// The execution class state (ADR-0037 Decision 9)
// ---------------------------------------------------------------------------------------------

/// The closed class status set. Explicit discriminants because this enum is chain state:
/// a reordering must never silently renumber what pruning-surviving bytes mean.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwClassStatusV3 {
    /// Registered, not yet in the qualification ladder.
    Inactive = 0,
    /// In qualification (ADR-0028's ladder is the process; this is its on-chain shadow).
    Probation = 1,
    /// Passing all six consensus paths — the only status that does.
    Active = 2,
    /// Auto- or governance-frozen: new jobs and new credit halt, running disputes continue,
    /// the base chain continues, and (ADR-0038 Decision D) the class leaves the difficulty
    /// domain set. `freeze_reason` records why.
    Frozen = 3,
    /// Retired for good; re-activation requires a NEW class version, never this row.
    Deprecated = 4,
}

/// ADR-0037 Decision 9: pruning-surviving class state (compile-time fork parameters are how
/// the audit's flag-day bugs happened). Artifact roots are content hashes re-derived from
/// actual bytes at startup (the B8/B15 pattern) — a manifest mismatch refuses the class
/// upstream; this object only carries the pinned identities.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwExecutionClassStateV3 {
    /// The exact execution class (ADR-0034; a class, never a family).
    pub class_id: Hash64,
    /// The model band this class serves (ADR-0034 band).
    pub model_band_id: Hash64,
    pub status: PalwClassStatusV3,
    /// Root of the full artifact manifest (weights, tokenizer, template, runtime, flags…).
    pub manifest_root: Hash64,
    pub weights_root: Hash64,
    pub tokenizer_root: Hash64,
    pub runtime_binary_root: Hash64,
    /// The court's kernel catalog for this class — what A4's coverage was proven against.
    pub kernel_catalog_root: Hash64,
    /// Committee shape: panel size n…
    pub committee_size: u8,
    /// …and quorum q of the q-of-n sampled verification (ADR-0038 Decision C's alarm).
    pub quorum: u8,
    /// The epoch at which `Active` took effect (staged activation, never a flag day).
    pub activation_epoch: u64,
    /// Why the class froze — `Some` iff a freeze happened; recorded, never inferred.
    pub freeze_reason: Option<Hash64>,
}

/// The six consensus paths that MUST consult class status (ADR-0037 Decision 9 verbatim).
/// Dispute continuation is deliberately NOT here: a freeze never stops a running dispute.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwClassPathV1 {
    JobAdmission,
    MinerCommitment,
    PanelSelection,
    AttestationAdmission,
    ProvisionalFinalization,
    CreditGeneration,
}

impl PalwExecutionClassStateV3 {
    /// Only an `Active` class passes ANY of the six paths. `Probation` is not a partial
    /// pass — the qualification ladder runs off-path, and "almost active" admitting jobs
    /// is exactly the fail-open the six-path gate exists to end.
    pub fn check_path(&self, path: PalwClassPathV1) -> Result<(), PalwDisputeError> {
        match self.status {
            PalwClassStatusV3::Active => Ok(()),
            status => Err(PalwDisputeError::ClassNotActiveForPath { status, path }),
        }
    }

    /// The closed status machine:
    ///
    /// * `Inactive → Probation` — enters qualification.
    /// * `Probation → Active` — REQUIRES a coverage certificate whose class id matches
    ///   (ADR-0038 A4): `None` refuses with [`PalwDisputeError::ActivationNeedsCoverage`],
    ///   a foreign certificate with [`PalwDisputeError::CoverageCertificateClassMismatch`].
    /// * `Active → Frozen` and `Probation → Frozen` — REQUIRE `freeze_reason = Some`; the
    ///   reason is recorded onto `self.freeze_reason`. `None` refuses with
    ///   [`PalwDisputeError::FreezeNeedsReason`].
    /// * `Active → Deprecated`, `Frozen → Deprecated` — retirement.
    ///
    /// Everything else — including any exit from `Deprecated` and any thaw of `Frozen`
    /// (re-activation is a NEW class version, per Decision 2's re-audit rule) — refuses
    /// with [`PalwDisputeError::IllegalClassTransition`]. A refusal leaves `self` untouched.
    pub fn transition(
        &mut self,
        to: PalwClassStatusV3,
        coverage: Option<&PalwCatalogCoverageCertificateV1>,
        freeze_reason: Option<Hash64>,
    ) -> Result<(), PalwDisputeError> {
        use PalwClassStatusV3 as C;
        match (self.status, to) {
            (C::Inactive, C::Probation) => {}
            (C::Probation, C::Active) => {
                let cert = coverage.ok_or(PalwDisputeError::ActivationNeedsCoverage { class_id: self.class_id })?;
                if cert.execution_class_id != self.class_id {
                    return Err(PalwDisputeError::CoverageCertificateClassMismatch {
                        class_id: self.class_id,
                        certificate_class_id: cert.execution_class_id,
                    });
                }
            }
            (C::Active | C::Probation, C::Frozen) => {
                let reason = freeze_reason.ok_or(PalwDisputeError::FreezeNeedsReason)?;
                self.freeze_reason = Some(reason);
            }
            (C::Active | C::Frozen, C::Deprecated) => {}
            (from, to) => return Err(PalwDisputeError::IllegalClassTransition { from, to }),
        }
        self.status = to;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_catalog_coverage::{verify_catalog_coverage_v1, PalwReachableKernelSetV1};
    use PalwChallengerBondDispositionV3 as B;
    use PalwClassStatusV3 as C;
    use PalwJobStatusV3 as S;

    fn h64(word: u64) -> Hash64 {
        Hash64::from_u64_word(word)
    }

    /// Every status the job spine can inhabit, both polarities where one exists.
    fn all_job_statuses() -> Vec<S> {
        let mut v = vec![
            S::Open,
            S::Committed,
            S::PanelSelected,
            S::FinalizedAccepted,
            S::FinalizedRejected,
            S::Convicted,
            S::NoFaultFound,
            S::Unadjudicable,
        ];
        for accepted in [false, true] {
            v.push(S::Provisional { accepted });
            v.push(S::ChallengeWindow { provisionally_accepted: accepted });
            v.push(S::Disputed { provisionally_accepted: accepted });
            v.push(S::Adjudicating { provisionally_accepted: accepted });
        }
        v
    }

    fn all_class_statuses() -> [C; 5] {
        [C::Inactive, C::Probation, C::Active, C::Frozen, C::Deprecated]
    }

    fn all_paths() -> [PalwClassPathV1; 6] {
        use PalwClassPathV1 as P;
        [P::JobAdmission, P::MinerCommitment, P::PanelSelection, P::AttestationAdmission, P::ProvisionalFinalization, P::CreditGeneration]
    }

    fn class_state(class: u64, status: C) -> PalwExecutionClassStateV3 {
        PalwExecutionClassStateV3 {
            class_id: h64(class),
            model_band_id: h64(0xBA),
            status,
            manifest_root: h64(1),
            weights_root: h64(2),
            tokenizer_root: h64(3),
            runtime_binary_root: h64(4),
            kernel_catalog_root: h64(5),
            committee_size: 7,
            quorum: 5,
            activation_epoch: 0,
            freeze_reason: None,
        }
    }

    /// A real coverage certificate for `class`, obtained the only way one can be: through
    /// the A4 gate with matching sets.
    fn certificate_for(class: u64) -> PalwCatalogCoverageCertificateV1 {
        // Real catalogued descriptors: the gate now reads the build's own table, so a fixture
        // has to name kernels this build can actually adjudicate.
        let reachable = PalwReachableKernelSetV1 {
            execution_class_id: h64(class),
            kernel_ids: crate::palw_step_refute::KDESC_BASE0_ALL[..3]
                .iter()
                .map(|d| crate::palw_step::kernel_semantics_id_v1(d))
                .collect(),
        };
        verify_catalog_coverage_v1(&reachable).unwrap()
    }

    /// The settlement table, exhaustively: the three court terminals produce EXACTLY the
    /// documented settlements (all four fields asserted each), and every OTHER status —
    /// both polarities of the parameterized ones and the non-court terminals
    /// `FinalizedAccepted`/`FinalizedRejected` included — refuses with `NotACourtTerminal`.
    #[test]
    fn settlement_table_is_exactly_three_rows_and_refuses_everything_else() {
        for status in all_job_statuses() {
            match status {
                S::Convicted => {
                    let s = settle_dispute_v3(status).unwrap();
                    assert!(s.slash_executor);
                    assert_eq!(s.challenger_bond, B::Refunded);
                    assert!(!s.job_creditable);
                    assert!(!s.freeze_class);
                }
                S::NoFaultFound => {
                    let s = settle_dispute_v3(status).unwrap();
                    assert!(!s.slash_executor);
                    assert_eq!(s.challenger_bond, B::Forfeited);
                    assert!(s.job_creditable);
                    assert!(!s.freeze_class);
                }
                S::Unadjudicable => {
                    let s = settle_dispute_v3(status).unwrap();
                    assert!(!s.slash_executor);
                    assert_eq!(s.challenger_bond, B::Refunded);
                    assert!(!s.job_creditable);
                    assert!(s.freeze_class);
                }
                other => {
                    assert_eq!(settle_dispute_v3(other), Err(PalwDisputeError::NotACourtTerminal { status: other }), "{other:?} settled");
                }
            }
        }
    }

    /// I10 pinned: `Unadjudicable` slashes NOBODY — executor unslashed AND challenger
    /// refunded (the class's catalog gap is not the challenger's fault) — and freezes the
    /// class. The map agrees with the spine's own predicates.
    #[test]
    fn i10_unadjudicable_slashes_nobody_and_freezes_the_class() {
        let s = settle_dispute_v3(S::Unadjudicable).unwrap();
        assert!(!s.slash_executor);
        assert_eq!(s.challenger_bond, B::Refunded);
        assert!(s.freeze_class);
        assert_eq!(s.slash_executor, S::Unadjudicable.slashes_executor());
        assert_eq!(s.job_creditable, S::Unadjudicable.creditable());
        assert_eq!(s.freeze_class, S::Unadjudicable.demands_class_freeze());
    }

    /// I9 pinned: `slash_executor` is true for `Convicted` and ONLY `Convicted` across all
    /// statuses — every other status either settles without a slash or refuses to settle.
    #[test]
    fn i9_only_convicted_slashes_the_executor() {
        for status in all_job_statuses() {
            let slashes = matches!(settle_dispute_v3(status), Ok(s) if s.slash_executor);
            assert_eq!(slashes, status == S::Convicted, "slash polarity wrong for {status:?}");
        }
    }

    /// The six-path gate: `Active` passes all six paths; every other status refuses all six,
    /// naming both the status and the refused path.
    #[test]
    fn check_path_passes_only_active_on_all_six_paths() {
        for status in all_class_statuses() {
            let cls = class_state(0xC1, status);
            for path in all_paths() {
                let got = cls.check_path(path);
                if status == C::Active {
                    assert_eq!(got, Ok(()), "Active refused {path:?}");
                } else {
                    assert_eq!(got, Err(PalwDisputeError::ClassNotActiveForPath { status, path }), "{status:?} passed {path:?}");
                }
            }
        }
    }

    /// The class machine's happy path: Inactive → Probation → Active (with a matching
    /// certificate) → Frozen (with a reason, recorded onto `freeze_reason`) → Deprecated.
    #[test]
    fn class_machine_happy_path_records_the_freeze_reason() {
        let mut cls = class_state(0xC1, C::Inactive);
        cls.transition(C::Probation, None, None).unwrap();
        assert_eq!(cls.status, C::Probation);
        cls.transition(C::Active, Some(&certificate_for(0xC1)), None).unwrap();
        assert_eq!(cls.status, C::Active);
        assert_eq!(cls.freeze_reason, None);
        cls.transition(C::Frozen, None, Some(h64(0xF0))).unwrap();
        assert_eq!(cls.status, C::Frozen);
        assert_eq!(cls.freeze_reason, Some(h64(0xF0)), "the freeze reason must be recorded");
        cls.transition(C::Deprecated, None, None).unwrap();
        assert_eq!(cls.status, C::Deprecated);
    }

    /// A4 at activation: no certificate refuses with `ActivationNeedsCoverage`; a real
    /// certificate for a DIFFERENT class refuses with `CoverageCertificateClassMismatch`.
    /// Both refusals leave the class in `Probation`.
    #[test]
    fn activation_demands_a_certificate_naming_this_class() {
        let mut cls = class_state(0xC1, C::Probation);
        assert_eq!(cls.transition(C::Active, None, None), Err(PalwDisputeError::ActivationNeedsCoverage { class_id: h64(0xC1) }));
        assert_eq!(cls.status, C::Probation);
        let foreign = certificate_for(0xC2);
        assert_eq!(
            cls.transition(C::Active, Some(&foreign), None),
            Err(PalwDisputeError::CoverageCertificateClassMismatch { class_id: h64(0xC1), certificate_class_id: h64(0xC2) })
        );
        assert_eq!(cls.status, C::Probation);
    }

    /// Freeze without a reason refuses with `FreezeNeedsReason` (from both freezable
    /// statuses), and the illegal jumps refuse with `IllegalClassTransition`: no skipping
    /// qualification (Inactive → Active), no exit from `Deprecated`, no thaw of `Frozen`.
    #[test]
    fn freeze_needs_reason_and_illegal_jumps_refuse() {
        for from in [C::Probation, C::Active] {
            let mut cls = class_state(0xC1, from);
            assert_eq!(cls.transition(C::Frozen, None, None), Err(PalwDisputeError::FreezeNeedsReason));
            assert_eq!(cls.status, from, "a refused freeze must not move the status");
            assert_eq!(cls.freeze_reason, None, "a refused freeze must not record a reason");
        }
        let mut cls = class_state(0xC1, C::Inactive);
        assert_eq!(
            cls.transition(C::Active, Some(&certificate_for(0xC1)), None),
            Err(PalwDisputeError::IllegalClassTransition { from: C::Inactive, to: C::Active })
        );
        for to in all_class_statuses() {
            let mut cls = class_state(0xC1, C::Deprecated);
            assert_eq!(
                cls.transition(to, Some(&certificate_for(0xC1)), Some(h64(0xF0))),
                Err(PalwDisputeError::IllegalClassTransition { from: C::Deprecated, to }),
                "Deprecated escaped to {to:?}"
            );
        }
        let mut cls = class_state(0xC1, C::Frozen);
        assert_eq!(
            cls.transition(C::Active, Some(&certificate_for(0xC1)), None),
            Err(PalwDisputeError::IllegalClassTransition { from: C::Frozen, to: C::Active }),
            "a frozen class must not thaw — re-activation is a new class version"
        );
    }

    /// Borsh roundtrips: the dispute state and the class state (both with and without a
    /// recorded freeze reason) survive serialize → deserialize compare-equal.
    #[test]
    fn borsh_roundtrips_dispute_state_and_class_state() {
        let dispute = PalwDisputeStateV3 {
            job_id: h64(1),
            challenger_bond_outpoint: TransactionOutpoint::new(h64(2), 3),
            refutation_evidence_id: h64(4),
            filed_daa: 5,
            adjudication_deadline: PalwDualDeadlineV3 { due_daa: 6, due_mtp: 7 },
            challenger_bond_amount: 8,
        };
        let dispute2: PalwDisputeStateV3 = borsh::from_slice(&borsh::to_vec(&dispute).unwrap()).unwrap();
        assert_eq!(dispute, dispute2);

        let mut cls = class_state(0xC1, C::Active);
        let cls2: PalwExecutionClassStateV3 = borsh::from_slice(&borsh::to_vec(&cls).unwrap()).unwrap();
        assert_eq!(cls, cls2);
        cls.transition(C::Frozen, None, Some(h64(0xF0))).unwrap();
        let cls3: PalwExecutionClassStateV3 = borsh::from_slice(&borsh::to_vec(&cls).unwrap()).unwrap();
        assert_eq!(cls, cls3);
    }

    /// Dual-deadline integration: the adjudication deadline reuses the spine's type, so
    /// expiry requires BOTH clocks — DAA alone or MTP alone is not an expired court window.
    #[test]
    fn adjudication_deadline_requires_both_clocks() {
        let deadline = PalwDualDeadlineV3 { due_daa: 200, due_mtp: 9_000 };
        assert!(!deadline.expired(200, 8_999), "DAA alone must not expire the court's window");
        assert!(deadline.expired(200, 9_000));
    }
}
