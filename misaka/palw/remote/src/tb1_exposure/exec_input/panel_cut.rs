//! Non-paying local PANEL intake. The holder re-reads every named receipt and
//! the later scheduled close on its selected chain. Pool eligibility and
//! reservation references are supplied by the enclosing private policy.
use super::{original::read_published_qf1_original, *};
use crate::tb1_exposure::Tb1HolderFreeze;
use crate::tb1_exposure::publication::with_published_batch;

pub struct LocalSubmission<'a, 't> {
    pub input: &'a DerivedExecInput<'t>,
    pub block: Hash64,
    pub body: &'a LocalOriginalBody,
    pub signature: &'a MLDSA65Signature,
    pub material_reservation: Hash64,
}

/// Original evidence read from retained receipts, independent of a live
/// DerivedExecInput. The slot's prompt is reconstructed from the EXEC seed.
#[derive(Clone, Copy)]
pub struct RecordedLocalSubmission<'a> {
    pub slot: u64,
    pub block: Hash64,
    pub body: &'a LocalOriginalBody,
    pub signature: &'a MLDSA65Signature,
    pub material_reservation: Hash64,
}

#[derive(Clone, Copy)]
pub struct PublishedLocalSubmission {
    pub slot: u64,
    pub block: Hash64,
    pub material_reservation: Hash64,
}

type SuppliedOriginal<'a> = (&'a LocalOriginalBody, &'a MLDSA65Signature);

pub struct LocalPanelInputs<'a> {
    /// Policy ID of this private draw and its duties. Eligibility is resolved
    /// outside the cut; the canonical candidate IDs and weights are frozen here.
    pub draw_policy: Hash64,
    pub candidates: &'a [(Hash64, u64)],
    pub max_candidates: usize,
}

struct FrozenReceipt {
    original: Hash64,
    block: Hash64,
    meets_target: bool,
    deadline: u64,
}

pub struct LocalPanelCut {
    closure: Tb1MiningClosure,
    exec_close: Hash64,
    certificate: Hash64,
    receipts: Vec<FrozenReceipt>,
    candidates: Vec<(Hash64, u64)>,
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
        if submissions.len() > self.batch.entries.len() {
            return Err(invalid("local PANEL cut exceeds its reservation"));
        }
        let tip = native.get_sink();
        let mut recorded = Vec::with_capacity(submissions.len());
        for row in submissions {
            let input = row.input;
            let entry = self.batch.entries.get(&input.slot).ok_or_else(|| invalid("unknown local PANEL registration slot"))?;
            if input.close != self.close || input.certificate != certificate || input.registration_key != reg_key(&entry.context) {
                return Err(invalid("local PANEL Original outside frozen EXEC allocation"));
            }
            recorded.push(RecordedLocalSubmission {
                slot: input.slot,
                block: row.block,
                body: row.body,
                signature: row.signature,
                material_reservation: row.material_reservation,
            });
        }
        let cut = self.recorded_panel_cut(native, publication, certificate, releases, q, limits, &recorded, pool)?;
        for row in submissions {
            row.input.check_live(native)?;
        }
        self.gate.require_tip(native, tip)?;
        Ok(cut)
    }

    /// Rebuild the committed consumer-set preimage from the original selected
    /// receipts. Each receipt must have landed in its job's signed window.
    /// Replay signing and new PANEL publication still use their live checks.
    #[allow(clippy::too_many_arguments)]
    pub fn recorded_panel_cut(
        &self,
        native: &dyn ConsensusApi,
        publication: Hash64,
        certificate: Hash64,
        releases: &[PublishedRelease<'_>],
        q: &Tb1Qualification<'_>,
        limits: PublicationLimits,
        submissions: &[RecordedLocalSubmission<'_>],
        pool: LocalPanelInputs<'_>,
    ) -> io::Result<LocalPanelCut> {
        let receipts = submissions.iter().map(|row| {
            let reference =
                PublishedLocalSubmission { slot: row.slot, block: row.block, material_reservation: row.material_reservation };
            (reference, Some((row.body, row.signature)))
        });
        self.panel_cut_from_receipts(native, publication, certificate, releases, q, limits, receipts, pool)
    }

    /// Rebuild the PANEL cut from the Original carriers in its selected
    /// branch. The caller supplies receipt coordinates and material reservations.
    #[allow(clippy::too_many_arguments)]
    pub fn published_panel_cut(
        &self,
        native: &dyn ConsensusApi,
        publication: Hash64,
        certificate: Hash64,
        releases: &[PublishedRelease<'_>],
        q: &Tb1Qualification<'_>,
        limits: PublicationLimits,
        submissions: &[PublishedLocalSubmission],
        pool: LocalPanelInputs<'_>,
    ) -> io::Result<LocalPanelCut> {
        self.panel_cut_from_receipts(
            native,
            publication,
            certificate,
            releases,
            q,
            limits,
            submissions.iter().map(|row| (*row, None)),
            pool,
        )
    }

    /// Read both the EXEC share batch and the Original carriers from the chain.
    #[allow(clippy::too_many_arguments)]
    pub fn panel_cut_from_published_blocks(
        &self,
        native: &dyn ConsensusApi,
        publication: Hash64,
        certificate: Hash64,
        q: &Tb1Qualification<'_>,
        limits: PublicationLimits,
        submissions: &[PublishedLocalSubmission],
        pool: LocalPanelInputs<'_>,
    ) -> io::Result<LocalPanelCut> {
        with_published_batch(native, publication, limits, |releases| {
            self.published_panel_cut(native, publication, certificate, releases, q, limits, submissions, pool)
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn panel_cut_from_receipts<'a>(
        &self,
        native: &dyn ConsensusApi,
        publication: Hash64,
        certificate: Hash64,
        releases: &[PublishedRelease<'_>],
        q: &Tb1Qualification<'_>,
        limits: PublicationLimits,
        submissions: impl ExactSizeIterator<Item = (PublishedLocalSubmission, Option<SuppliedOriginal<'a>>)>,
        pool: LocalPanelInputs<'_>,
    ) -> io::Result<LocalPanelCut> {
        if pool.candidates.len() > pool.max_candidates || submissions.len() > self.batch.entries.len() {
            return Err(invalid("local PANEL cut exceeds its reservation"));
        }
        let tip = native.get_sink();
        let exec_seed = self.gate.read_seed(native, publication, certificate, releases, q, limits)?;
        let rounds = &self.batch.rounds;
        let score = rounds.panel().close_blue_score;
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
        for (row, supplied) in submissions {
            let entry = self.batch.entries.get(&row.slot).ok_or_else(|| invalid("unknown local PANEL registration slot"))?;
            if previous.is_some_and(|slot| slot >= row.slot)
                || native.get_header(row.block).map_err(io::Error::other)?.blue_score >= score
            {
                return Err(invalid("local PANEL Original outside frozen EXEC allocation or receipt cutoff"));
            }
            let r = self.derive_from_seed(row.slot, certificate, &exec_seed)?;
            let published;
            let (body, signature) = match supplied {
                Some(evidence) => evidence,
                None => {
                    published = read_published_qf1_original(native, row.block)?;
                    (&published.0, &published.1)
                }
            };
            let original = r.read_recorded_qf1_original(native, row.block, body, signature)?;
            hash.update(row.slot.to_be_bytes());
            hash.update(r.registration_key);
            hash.update(original.as_bytes());
            hash.update(row.block.as_bytes());
            hash.update(entry.funding_ref);
            hash.update(row.material_reservation.as_bytes());
            hash.update(entry.terms.deadline_exclusive_daa.to_be_bytes());
            previous = Some(row.slot);
            receipts.push(FrozenReceipt {
                original,
                block: row.block,
                meets_target: r.terms.threshold()?.admits_ticket_be(&body.ticket),
                deadline: entry.terms.deadline_exclusive_daa,
            });
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
            round: *rounds.panel(),
            qual_root: rounds.panel_qual(),
            consumer_set_root: hash.finalize().into(),
            policy_root: pool.draw_policy.as_bytes(),
        };
        self.gate.require_tip(native, tip)?;
        Ok(LocalPanelCut { closure, exec_close: self.close, certificate, receipts, candidates: pool.candidates.to_vec() })
    }
}

impl LocalPanelCut {
    /// Committed consumer-set digest for retaining the public closure preimage.
    pub fn consumer_set_root(&self) -> [u8; 64] {
        self.closure.consumer_set_root
    }

    pub fn commitment(&self, parent: Hash64) -> [u8; 64] {
        self.closure.commitment(parent)
    }
    pub fn resolve(&self, native: &dyn ConsensusApi, close: Hash64, q: &Tb1Qualification<'_>) -> io::Result<Tb1HolderFreeze> {
        let (freeze, daa) = self.resolve_on_selected_branch(native, close, q)?;
        if daa >= self.closure.round.deadline_exclusive_daa {
            return Err(invalid("local PANEL close outside live EXEC history"));
        }
        Ok(freeze)
    }

    fn resolve_on_selected_branch(
        &self,
        native: &dyn ConsensusApi,
        close: Hash64,
        q: &Tb1Qualification<'_>,
    ) -> io::Result<(Tb1HolderFreeze, u64)> {
        let (state, daa, tip) = anchor_state(native, self.exec_close)?;
        let parent = native.get_ghostdag_data(close).map_err(io::Error::other)?.selected_parent;
        if !matches!(state, AnchorState::Finalized)
            || !native.is_chain_ancestor_of(self.certificate, parent).map_err(io::Error::other)?
            || !native.is_chain_ancestor_of(close, tip).map_err(io::Error::other)?
        {
            return Err(invalid("local PANEL close outside live EXEC history"));
        }
        for receipt in &self.receipts {
            if !native.is_chain_ancestor_of(receipt.block, parent).map_err(io::Error::other)? {
                return Err(invalid("local Original absent from PANEL prefix"));
            }
        }
        let freeze = Tb1HolderFreeze::resolve(native, close, &self.closure, q)?;
        let (exec_state, _, exec_tip) = anchor_state(native, self.exec_close)?;
        if exec_tip != tip || !matches!(exec_state, AnchorState::Finalized) {
            return Err(io::Error::new(io::ErrorKind::WouldBlock, "local PANEL close view moved"));
        }
        Ok((freeze, daa))
    }
    pub fn observe(&self, native: &dyn ConsensusApi, close: Hash64, q: &Tb1Qualification<'_>) -> io::Result<NativePublicationGate> {
        self.resolve(native, close, q)?;
        NativePublicationGate::observe(native, close, &self.closure, q)
    }
}

#[path = "panel_duty.rs"]
mod duty;
pub use duty::{LocalReplayDraw, LocalReplayPolicy, REPLAY_SIGNATURE_DOMAIN, actor_id};
