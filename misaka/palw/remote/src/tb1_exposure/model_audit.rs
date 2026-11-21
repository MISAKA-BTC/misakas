//! Private RFC-0065 pre-evaluation cut. The owning profile supplies the suite,
//! generation rule, reserved capacity and candidate budget before disclosure.
//! This reader produces evaluation challenges only.
#[cfg(feature = "private-tb1-tir")]
pub mod arithmetic;

use super::{
    Tb1HolderFreeze, check_panel_schedule, invalid,
    publication::{NativePublicationGate, PublicationLimits, PublishedRelease},
};
use kaspa_consensus_core::{
    Hash64,
    api::ConsensusApi,
    palw_state_proof_v1::{PALW_PROOF_LABEL_CLASSES, state_root_of_pinned_header_v1, verify_class_v1},
    palw_tb1::{
        freeze::{Tb1MiningClosure, Tb1MiningPurpose, Tb1MiningRound},
        qualification::Tb1Qualification,
    },
};
use sha3::{Digest, Sha3_256, Sha3_512};
use std::io;

#[derive(Clone, Copy)]
pub struct AuditCandidate {
    pub class: Hash64,
    pub artifact: Hash64,
    pub descriptor: Hash64,
    pub generation: u64,
}

#[derive(Clone, Copy)]
pub struct AuditPlan {
    /// The immutable suite includes the scoring rule, thresholds and input generator.
    pub suite: Hash64,
    pub candidate_budget: u32,
    pub tests_per_candidate: u32,
    /// Reservation for the full candidate set and its evaluation cases.
    pub reservation: Hash64,
    pub case_deadline_exclusive_daa: u64,
}

#[derive(Clone)]
pub struct PreparedAudit<'a> {
    preparation: Hash64,
    candidates: &'a [AuditCandidate],
    plan: AuditPlan,
    closure: Tb1MiningClosure,
    #[cfg(feature = "private-tb1-tir")]
    panel_close_blue_score: u64,
}

impl<'a> PreparedAudit<'a> {
    /// The preparation marker commits to both pre-qualified rounds before the
    /// close. An audit is always paired with its own later evaluation PANEL.
    pub fn preparation_marker(
        audit: &Tb1MiningRound,
        audit_q: &Tb1Qualification<'_>,
        panel: &Tb1MiningRound,
        panel_q: &Tb1Qualification<'_>,
    ) -> io::Result<[u8; 64]> {
        check_panel_schedule(Tb1MiningPurpose::ModelAudit, audit, audit_q, panel, panel_q)?;
        Ok(Sha3_512::new()
            .chain_update(b"MISAKA/private/MAS1/qualified-evaluation-rounds/v1")
            .chain_update(audit.id())
            .chain_update(audit_q.root())
            .chain_update(panel.id())
            .chain_update(panel_q.root())
            .finalize()
            .into())
    }

    pub fn read(
        native: &dyn ConsensusApi,
        preparation: Hash64,
        audit: Tb1MiningRound,
        audit_q: &Tb1Qualification<'_>,
        panel: &Tb1MiningRound,
        panel_q: &Tb1Qualification<'_>,
        plan: AuditPlan,
        candidates: &'a [AuditCandidate],
    ) -> io::Result<Self> {
        let rounds = Self::preparation_marker(&audit, audit_q, panel, panel_q)?;
        let count = u32::try_from(candidates.len()).map_err(|_| invalid("oversized MAS1 candidate set"))?;
        if count == 0
            || count > plan.candidate_budget
            || plan.tests_per_candidate == 0
            || plan.suite == Hash64::default()
            || plan.reservation == Hash64::default()
            || panel.deadline_exclusive_daa > plan.case_deadline_exclusive_daa
            || audit.deadline_exclusive_daa > plan.case_deadline_exclusive_daa
            || candidates
                .windows(2)
                .any(|w| (w[0].class, w[0].descriptor, w[0].generation) >= (w[1].class, w[1].descriptor, w[1].generation))
        {
            return Err(invalid("invalid MAS1 candidate set, bounds or deadlines"));
        }
        let tip = native.get_sink();
        if !native.is_chain_ancestor_of(preparation, tip).map_err(io::Error::other)?
            || !native.palw_coinbase_commits_to(audit.chain_id, preparation, &rounds).map_err(io::Error::other)?
        {
            return Err(invalid("MAS1 qualification is outside the prepared history"));
        }
        let (header, classes) = native.palw_state_proof_v1(preparation, PALW_PROOF_LABEL_CLASSES).map_err(io::Error::other)?;
        let state = state_root_of_pinned_header_v1(&header, preparation).map_err(io::Error::other)?;
        if header.blue_score >= audit.close_blue_score {
            return Err(invalid("MAS1 preparation must precede the candidate close"));
        }
        let mut h = Sha3_512::new()
            .chain_update(b"MISAKA/private/MAS1/non-rewarding-candidates/v1")
            .chain_update(preparation.as_bytes())
            .chain_update(rounds)
            .chain_update(plan.suite.as_bytes())
            .chain_update(plan.reservation.as_bytes())
            .chain_update(plan.candidate_budget.to_be_bytes())
            .chain_update(plan.tests_per_candidate.to_be_bytes())
            .chain_update(plan.case_deadline_exclusive_daa.to_be_bytes())
            .chain_update(count.to_be_bytes());
        for c in candidates {
            let row = verify_class_v1(&classes, state, &c.class).map_err(io::Error::other)?;
            if row.artifact_root != c.artifact || c.descriptor == Hash64::default() {
                return Err(invalid("MAS1 candidate identity mismatch"));
            }
            h.update(c.class.as_bytes());
            h.update(c.artifact.as_bytes());
            h.update(c.descriptor.as_bytes());
            h.update(c.generation.to_be_bytes());
        }
        if native.get_sink() != tip {
            return Err(io::Error::new(io::ErrorKind::WouldBlock, "MAS1 preparation view moved"));
        }
        let closure = Tb1MiningClosure {
            round: audit,
            qual_root: audit_q.root(),
            consumer_set_root: h.finalize().into(),
            policy_root: plan.suite.as_bytes(),
        };
        Ok(Self {
            preparation,
            candidates,
            plan,
            closure,
            #[cfg(feature = "private-tb1-tir")]
            panel_close_blue_score: panel.close_blue_score,
        })
    }

    pub fn close_commitment(&self, parent: Hash64) -> [u8; 64] {
        self.closure.commitment(parent)
    }

    fn check_ancestry(&self, native: &dyn ConsensusApi, close: Hash64) -> io::Result<()> {
        let parent = native.get_ghostdag_data(close).map_err(io::Error::other)?.selected_parent;
        if !native.is_chain_ancestor_of(self.preparation, parent).map_err(io::Error::other)? {
            return Err(invalid("MAS1 close outside prepared history"));
        }
        Ok(())
    }

    pub fn holder_freeze(&self, native: &dyn ConsensusApi, close: Hash64, q: &Tb1Qualification<'_>) -> io::Result<Tb1HolderFreeze> {
        self.check_ancestry(native, close)?;
        Tb1HolderFreeze::resolve(native, close, &self.closure, q)
    }

    /// Recheck ancestry and the native publication gate for each observation. The
    /// candidate set was authenticated at preparation, within the state proof window.
    pub fn observe(&self, native: &dyn ConsensusApi, close: Hash64, q: &Tb1Qualification<'_>) -> io::Result<FrozenAudit<'a>> {
        self.check_ancestry(native, close)?;
        let gate = NativePublicationGate::observe(native, close, &self.closure, q)?;
        Ok(FrozenAudit { cut: self.clone(), gate })
    }
}

pub struct FrozenAudit<'a> {
    cut: PreparedAudit<'a>,
    gate: NativePublicationGate,
}

impl FrozenAudit<'_> {
    pub fn publication_gate(&self) -> &NativePublicationGate {
        &self.gate
    }

    /// An evaluator may expand this into suite inputs only after reading the
    /// signed releases and certificate from its own selected native history.
    pub fn challenge(
        &self,
        native: &dyn ConsensusApi,
        batch: Hash64,
        certificate: Hash64,
        rows: &[PublishedRelease<'_>],
        q: &Tb1Qualification<'_>,
        limits: PublicationLimits,
        candidate: u32,
        case: u32,
    ) -> io::Result<[u8; 32]> {
        if candidate as usize >= self.cut.candidates.len() || case >= self.cut.plan.tests_per_candidate {
            return Err(invalid("MAS1 evaluation outside frozen budget"));
        }
        let tip = native.get_sink();
        if native.get_header(tip).map_err(io::Error::other)?.daa_score >= self.cut.plan.case_deadline_exclusive_daa {
            return Err(invalid("MAS1 evaluation deadline passed"));
        }
        let seed = self.gate.read_seed(native, batch, certificate, rows, q, limits)?;
        if native.get_sink() != tip {
            return Err(io::Error::new(io::ErrorKind::WouldBlock, "MAS1 evaluation view moved"));
        }
        Ok(Sha3_256::new()
            .chain_update(b"MISAKA/private/MAS1/evaluation-challenge/v1")
            .chain_update(self.cut.closure.consumer_set_root)
            .chain_update(seed.as_bytes())
            .chain_update(candidate.to_be_bytes())
            .chain_update(case.to_be_bytes())
            .finalize()
            .into())
    }
}
