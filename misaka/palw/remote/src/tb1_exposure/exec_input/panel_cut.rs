//! Non-paying local PANEL intake. The holder re-reads every named receipt and
//! the later scheduled close on its selected chain. Pool eligibility and
//! reservation references are supplied by the enclosing private policy.
use super::*;
use crate::tb1_exposure::Tb1HolderFreeze;

pub struct LocalSubmission<'a, 't> {
    pub input: &'a DerivedExecInput<'t>,
    pub block: Hash64,
    pub body: &'a LocalOriginalBody,
    pub signature: &'a MLDSA65Signature,
    pub material_reservation: Hash64,
}

pub struct LocalPanelInputs<'a> {
    /// Policy ID of this private draw and its duties. Eligibility is resolved
    /// outside the cut; the canonical candidate IDs and weights are frozen here.
    pub draw_policy: Hash64,
    pub candidates: &'a [(Hash64, u64)],
    pub max_candidates: usize,
}

pub struct LocalPanelCut {
    closure: Tb1MiningClosure,
    exec_close: Hash64,
    certificate: Hash64,
    receipts: Vec<Hash64>,
    deadline_exclusive: u64,
}

impl FrozenExecInputs<'_> {
    #[allow(clippy::too_many_arguments)]
    pub fn local_panel_cut(
        &self,
        native: &dyn ConsensusApi,
        publication: Hash64,
        certificate: Hash64,
        releases: &[PublishedRelease<'_>],
        q: &Tb1Qualification<'_>,
        limits: PublicationLimits,
        submissions: &[LocalSubmission<'_, '_>],
        pool: LocalPanelInputs<'_>,
    ) -> io::Result<LocalPanelCut> {
        if pool.candidates.len() > pool.max_candidates || submissions.len() > self.batch.entries.len() {
            return Err(invalid("local PANEL cut exceeds its reservation"));
        }
        let tip = native.get_sink();
        let exec_seed = self.gate.read_seed(native, publication, certificate, releases, q, limits)?;
        let rounds = &self.batch.rounds;
        let score = rounds.panel.close_blue_score;
        if native.get_header(certificate).map_err(io::Error::other)?.blue_score >= score {
            return Err(invalid("EXEC certificate after local PANEL cutoff"));
        }
        let mut hash = Sha3_512::new()
            .chain_update(b"MISAKA/private/QF1/local-panel-inputs/v1")
            .chain_update(self.batch.root)
            .chain_update(self.close.as_bytes())
            .chain_update(certificate.as_bytes())
            .chain_update(exec_seed.as_bytes())
            .chain_update(u32::try_from(submissions.len()).map_err(|_| invalid("PANEL submissions count"))?.to_be_bytes());
        let mut previous = None;
        let mut receipts = Vec::with_capacity(submissions.len());
        let mut deadline_exclusive = rounds.panel.deadline_exclusive_daa;
        for row in submissions {
            let r = row.input;
            let entry = self.batch.entries.get(&r.slot).ok_or_else(|| invalid("unknown local PANEL registration slot"))?;
            if previous.is_some_and(|slot| slot >= r.slot)
                || r.close != self.close
                || r.certificate != certificate
                || r.registration_key != reg_key(&entry.context)
                || native.get_header(row.block).map_err(io::Error::other)?.blue_score >= score
            {
                return Err(invalid("local PANEL Original outside frozen EXEC allocation or receipt cutoff"));
            }
            let original = r.read_qf1_original(native, row.block, row.body, row.signature)?;
            hash.update(r.slot.to_be_bytes());
            hash.update(r.registration_key);
            hash.update(original.as_bytes());
            hash.update(row.block.as_bytes());
            hash.update(entry.funding_ref);
            hash.update(row.material_reservation.as_bytes());
            hash.update(entry.terms.deadline_exclusive_daa.to_be_bytes());
            previous = Some(r.slot);
            deadline_exclusive = deadline_exclusive.min(entry.terms.deadline_exclusive_daa);
            receipts.push(row.block);
        }
        hash.update(u32::try_from(pool.candidates.len()).map_err(|_| invalid("PANEL pool count"))?.to_be_bytes());
        let mut previous = None;
        for &(actor, weight) in pool.candidates {
            if weight == 0 || previous.is_some_and(|p| p >= actor) {
                return Err(invalid("invalid local PANEL pool order or weight"));
            }
            hash.update(actor.as_bytes());
            hash.update(weight.to_be_bytes());
            previous = Some(actor);
        }
        let closure = Tb1MiningClosure {
            round: rounds.panel,
            qual_root: rounds.panel_qual,
            consumer_set_root: hash.finalize().into(),
            policy_root: pool.draw_policy.as_bytes(),
        };
        if native.get_sink() != tip {
            return Err(io::Error::new(io::ErrorKind::WouldBlock, "local PANEL input view moved"));
        }
        Ok(LocalPanelCut { closure, exec_close: self.close, certificate, receipts, deadline_exclusive })
    }
}

impl LocalPanelCut {
    pub fn commitment(&self, parent: Hash64) -> [u8; 64] {
        self.closure.commitment(parent)
    }
    pub fn resolve(&self, native: &dyn ConsensusApi, close: Hash64, q: &Tb1Qualification<'_>) -> io::Result<Tb1HolderFreeze> {
        let (state, daa, tip) = anchor_state(native, self.exec_close)?;
        let parent = native.get_ghostdag_data(close).map_err(io::Error::other)?.selected_parent;
        if !matches!(state, AnchorState::Finalized)
            || daa >= self.deadline_exclusive
            || !native.is_chain_ancestor_of(self.certificate, parent).map_err(io::Error::other)?
            || !native.is_chain_ancestor_of(close, tip).map_err(io::Error::other)?
        {
            return Err(invalid("local PANEL close outside live EXEC history"));
        }
        for &receipt in &self.receipts {
            if !native.is_chain_ancestor_of(receipt, parent).map_err(io::Error::other)? {
                return Err(invalid("local Original absent from PANEL prefix"));
            }
        }
        let freeze = Tb1HolderFreeze::resolve(native, close, &self.closure, q)?;
        if native.get_sink() != tip {
            return Err(io::Error::new(io::ErrorKind::WouldBlock, "local PANEL close view moved"));
        }
        Ok(freeze)
    }
    pub fn observe(&self, native: &dyn ConsensusApi, close: Hash64, q: &Tb1Qualification<'_>) -> io::Result<NativePublicationGate> {
        self.resolve(native, close, q)?;
        NativePublicationGate::observe(native, close, &self.closure, q)
    }
}
