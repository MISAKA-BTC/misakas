//! Private PANEL consumers. Candidates and complete-replay duties come from
//! the earlier native close. The dedicated certificate is the only draw input
//! that becomes known after that close. No positive vote grants mining credit.
use super::{original::read_published_qf1_original, *};
use kaspa_consensus_core::{
    palw_native_settlement_v1::NativeSettlementSnapshotV1,
    palw_tb1::{
        panel_reply_carrier::{
            PANEL_REPLY_KEY_BYTES, PANEL_REPLY_SIGNATURE_BYTES, PanelReplayCut, PanelReplayDuty, encode_panel_replies,
            panel_reply_commitment, replay_message, replay_policy_id,
        },
        releases::Tb1BeaconSeed,
    },
};
use misaka_palw_tir_exec::node::qf1::LocalInstance;

pub use kaspa_consensus_core::palw_tb1::panel_reply_carrier::{PublishedPanelApproval, REPLAY_SIGNATURE_DOMAIN, actor_id};

/// Uses the existing integer stake-ticket race with the frozen weights. This
/// private policy commits the seat count and all seats' complete replay duty.
#[derive(Clone, Copy)]
pub struct LocalReplayPolicy {
    seats: u32,
}
impl LocalReplayPolicy {
    pub fn new(seats: u32) -> io::Result<Self> {
        if seats == 0 {
            return Err(invalid("empty local PANEL"));
        }
        Ok(Self { seats })
    }
    /// Private evidence file, in draw order: one ML-DSA-65 public key and
    /// signature per seat. The native receipt authenticates the decoded rows.
    pub fn read_reply_evidence(self, mut input: impl io::Read) -> io::Result<Vec<(MLDSA65VerificationKey, MLDSA65Signature)>> {
        let mut rows = Vec::new();
        for _ in 0..self.seats {
            let mut key = [0; PANEL_REPLY_KEY_BYTES];
            let mut signature = [0; PANEL_REPLY_SIGNATURE_BYTES];
            input.read_exact(&mut key)?;
            input.read_exact(&mut signature)?;
            rows.push((MLDSA65VerificationKey::new(key), MLDSA65Signature::new(signature)));
        }
        if input.bytes().next().transpose()?.is_some() {
            return Err(invalid("extra local PANEL reply evidence"));
        }
        Ok(rows)
    }
    pub fn id(self) -> Hash64 {
        replay_policy_id(self.seats)
    }
}

pub struct LocalReplayDraw {
    chain: [u8; 64],
    original: Hash64,
    receipt: Hash64,
    close: Hash64,
    certificate: Hash64,
    deadline: u64,
    selected: Vec<Hash64>,
    duty: [u8; 64],
}

impl LocalPanelCut {
    #[allow(clippy::too_many_arguments)]
    pub fn draw_replays(
        &self,
        native: &dyn ConsensusApi,
        close: Hash64,
        gate: &NativePublicationGate,
        batch: Hash64,
        certificate: Hash64,
        rows: &[PublishedRelease<'_>],
        q: &Tb1Qualification<'_>,
        limits: PublicationLimits,
        original: Hash64,
        policy: LocalReplayPolicy,
    ) -> io::Result<LocalReplayDraw> {
        let tip = native.get_sink();
        let freeze = self.resolve(native, close, q)?;
        let draw = self.reconstruct_draw(close, gate, certificate, original, policy, freeze, || {
            gate.read_seed(native, batch, certificate, rows, q, limits)
        })?;
        draw.live(native)?;
        if native.get_sink() != tip {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        Ok(draw)
    }

    /// Draw the live PANEL seats from the shares recorded in the batch block.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_published_replays(
        &self,
        native: &dyn ConsensusApi,
        close: Hash64,
        gate: &NativePublicationGate,
        batch: Hash64,
        certificate: Hash64,
        q: &Tb1Qualification<'_>,
        limits: PublicationLimits,
        original: Hash64,
        policy: LocalReplayPolicy,
    ) -> io::Result<LocalReplayDraw> {
        let tip = native.get_sink();
        let freeze = self.resolve(native, close, q)?;
        let draw = self.reconstruct_draw(close, gate, certificate, original, policy, freeze, || {
            gate.read_published_seed(native, batch, certificate, q, limits)
        })?;
        draw.live(native)?;
        if native.get_sink() != tip {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        Ok(draw)
    }

    /// Recover the private duty after the live signing window. Callers still
    /// have to check the included reply or expiry against this selected branch.
    #[allow(clippy::too_many_arguments)]
    pub fn recorded_draw(
        &self,
        native: &dyn ConsensusApi,
        close: Hash64,
        gate: &NativePublicationGate,
        batch: Hash64,
        certificate: Hash64,
        rows: &[PublishedRelease<'_>],
        q: &Tb1Qualification<'_>,
        limits: PublicationLimits,
        original: Hash64,
        policy: LocalReplayPolicy,
    ) -> io::Result<LocalReplayDraw> {
        let tip = native.get_sink();
        let (freeze, _) = self.resolve_on_selected_branch(native, close, q)?;
        let draw = self.reconstruct_draw(close, gate, certificate, original, policy, freeze, || {
            gate.read_seed(native, batch, certificate, rows, q, limits)
        })?;
        if native.get_sink() != tip {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        Ok(draw)
    }

    /// Verify a previously included reply set from its retained cut and publication
    /// evidence, including when the original deadline has elapsed.
    #[allow(clippy::too_many_arguments)]
    pub fn read_recorded_replies(
        &self,
        native: &dyn ConsensusApi,
        close: Hash64,
        gate: &NativePublicationGate,
        batch: Hash64,
        certificate: Hash64,
        rows: &[PublishedRelease<'_>],
        q: &Tb1Qualification<'_>,
        limits: PublicationLimits,
        original: Hash64,
        policy: LocalReplayPolicy,
        block: Hash64,
        replies: &[(&MLDSA65VerificationKey, &MLDSA65Signature)],
    ) -> io::Result<()> {
        let tip = native.get_sink();
        let draw = self.recorded_draw(native, close, gate, batch, certificate, rows, q, limits, original, policy)?;
        draw.read_replies(native, block, replies)?;
        if native.get_sink() != tip {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        Ok(())
    }

    /// Reconstruct the draw and verify its published reply set for a
    /// candidate's authenticated selected parent. The accepting block must
    /// be timely even when the candidate follows the original deadline.
    #[allow(clippy::too_many_arguments)]
    pub fn read_published_approval_at_parent(
        &self,
        native: &dyn ConsensusApi,
        parent: Hash64,
        settlement: &NativeSettlementSnapshotV1,
        close: Hash64,
        gate: &NativePublicationGate,
        batch: Hash64,
        certificate: Hash64,
        q: &Tb1Qualification<'_>,
        limits: PublicationLimits,
        original: Hash64,
        policy: LocalReplayPolicy,
        block: Hash64,
    ) -> io::Result<PublishedPanelApproval> {
        let freeze = self.resolve_at_parent(native, close, parent, q)?;
        let draw = self.reconstruct_draw(close, gate, certificate, original, policy, freeze, || {
            gate.read_published_seed_at_parent(native, parent, settlement, batch, certificate, q, limits)
        })?;
        draw.read_published_replies_on_branch(native, parent, block)
    }

    #[allow(clippy::too_many_arguments)]
    fn reconstruct_draw(
        &self,
        close: Hash64,
        gate: &NativePublicationGate,
        certificate: Hash64,
        original: Hash64,
        policy: LocalReplayPolicy,
        freeze: Tb1HolderFreeze,
        read_seed: impl FnOnce() -> io::Result<Tb1BeaconSeed>,
    ) -> io::Result<LocalReplayDraw> {
        let draw = self.replay_cut(close, gate, original, policy, freeze)?.draw(certificate, read_seed()?)?;
        Ok(LocalReplayDraw {
            chain: draw.chain,
            original: draw.original,
            receipt: draw.original_block,
            close,
            certificate: draw.certificate,
            deadline: draw.deadline,
            selected: draw.selected,
            duty: draw.duty,
        })
    }

    fn replay_cut(
        &self,
        close: Hash64,
        gate: &NativePublicationGate,
        original: Hash64,
        policy: LocalReplayPolicy,
        freeze: Tb1HolderFreeze,
    ) -> io::Result<PanelReplayCut<'_>> {
        let receipt = self.receipts.iter().find(|r| r.original == original).ok_or_else(|| invalid("Original outside PANEL cut"))?;
        if !receipt.meets_target
            || policy.id().as_bytes() != self.closure.policy_root
            || gate.holder_freeze().accepting_block != close
            || gate.holder_freeze().freeze_id != freeze.freeze_id
        {
            return Err(invalid("different PANEL policy, Original or publication gate"));
        }
        Ok(PanelReplayCut {
            closure: &self.closure,
            close,
            freeze_id: freeze.freeze_id,
            original,
            original_block: receipt.block,
            deadline: receipt.deadline,
            candidates: &self.candidates,
            seats: policy.seats,
        })
    }

    /// Recover the block-owned draw from an in-flight PANEL certificate.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_replays_in_candidate(
        &self,
        native: &dyn ConsensusApi,
        close: Hash64,
        parent: Hash64,
        settlement: &NativeSettlementSnapshotV1,
        gate: &NativePublicationGate,
        batch: Hash64,
        candidate: &kaspa_consensus_core::block::Block,
        q: &Tb1Qualification<'_>,
        limits: PublicationLimits,
        original: Hash64,
        policy: LocalReplayPolicy,
    ) -> io::Result<PanelReplayDuty> {
        let freeze = self.resolve_at_parent(native, close, parent, q)?;
        let cut = self.replay_cut(close, gate, original, policy, freeze)?;
        let publication = gate.verify_published_candidate(native, parent, settlement, batch, candidate, q, limits)?;
        cut.draw_for_candidate(&publication)
    }
}
impl LocalReplayDraw {
    pub fn selected(&self) -> &[Hash64] {
        &self.selected
    }
    fn branch(&self, native: &dyn ConsensusApi) -> io::Result<(u64, Hash64)> {
        let (state, daa, tip) = anchor_state(native, self.close)?;
        if !matches!(state, AnchorState::Finalized) || !native.is_chain_ancestor_of(self.certificate, tip).map_err(io::Error::other)? {
            return Err(invalid("local PANEL duty outside live history"));
        }
        if native.get_sink() != tip {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        Ok((daa, tip))
    }
    fn live(&self, native: &dyn ConsensusApi) -> io::Result<()> {
        let (daa, _) = self.branch(native)?;
        if daa >= self.deadline {
            return Err(io::ErrorKind::TimedOut.into());
        }
        Ok(())
    }
    fn message(&self, key: &MLDSA65VerificationKey) -> io::Result<[u8; 64]> {
        let actor = actor_id(key);
        if !self.selected.contains(&actor) {
            return Err(invalid("actor not selected for local replay"));
        }
        Ok(replay_message(&self.duty, actor))
    }

    /// Read the assigned Original from its source block, then independently
    /// rerun the complete execution before producing the replay message.
    pub fn replay_published_message(
        &self,
        native: &dyn ConsensusApi,
        input: &DerivedExecInput<'_>,
        instance: &LocalInstance<'_>,
        key: &MLDSA65VerificationKey,
    ) -> io::Result<[u8; 64]> {
        self.live(native)?;
        let (body, owner) = read_published_qf1_original(native, self.receipt)?;
        self.replay_message(native, input, instance, &body, &owner, key)
    }

    /// This producer path executes the selected Original from the registered
    /// model, even if the caller already computed a trace before the draw.
    pub fn replay_message(
        &self,
        native: &dyn ConsensusApi,
        input: &DerivedExecInput<'_>,
        instance: &LocalInstance<'_>,
        body: &LocalOriginalBody,
        owner: &MLDSA65Signature,
        key: &MLDSA65VerificationKey,
    ) -> io::Result<[u8; 64]> {
        let msg = self.message(key)?;
        self.live(native)?;
        if input.read_qf1_original(native, self.receipt, body, owner)? != self.original {
            return Err(invalid("different PANEL Original"));
        }
        let len = u32::try_from(body.counts[1]).map_err(|_| invalid("answer length overflow"))?;
        let replay = input.replay_qf1_original(native, instance, len, None).map_err(io::Error::other)?;
        replay.verify(native, body, owner)?;
        self.live(native)?;
        Ok(msg)
    }
    pub fn verify_reply(&self, native: &dyn ConsensusApi, key: &MLDSA65VerificationKey, sig: &MLDSA65Signature) -> io::Result<()> {
        let msg = self.message(key)?;
        ml_dsa_65::verify(key, &msg, REPLAY_SIGNATURE_DOMAIN, sig).map_err(|_| invalid("invalid local PANEL reply"))?;
        self.live(native)
    }
}

impl LocalReplayDraw {
    /// All seats, in draw order. An omitted seat or repeated key cannot close the duty batch.
    pub fn reply_commitment(
        &self,
        native: &dyn ConsensusApi,
        replies: &[(&MLDSA65VerificationKey, &MLDSA65Signature)],
    ) -> io::Result<[u8; 64]> {
        let (daa, tip) = self.branch(native)?;
        if daa >= self.deadline {
            return Err(io::ErrorKind::TimedOut.into());
        }
        let marker = self.replies_marker(replies);
        if self.branch(native)?.1 != tip {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        marker
    }

    /// Prepare the existing commitment and its complete signed reply payload.
    pub fn publish_replies(
        &self,
        native: &dyn ConsensusApi,
        replies: &[(&MLDSA65VerificationKey, &MLDSA65Signature)],
    ) -> io::Result<([u8; 64], Vec<u8>)> {
        Ok((self.reply_commitment(native, replies)?, encode_panel_replies(replies)))
    }

    fn replies_marker(&self, replies: &[(&MLDSA65VerificationKey, &MLDSA65Signature)]) -> io::Result<[u8; 64]> {
        panel_reply_commitment(&self.duty, &self.selected, replies)
    }
    pub fn read_replies(
        &self,
        native: &dyn ConsensusApi,
        block: Hash64,
        replies: &[(&MLDSA65VerificationKey, &MLDSA65Signature)],
    ) -> io::Result<()> {
        let (_, tip) = self.branch(native)?;
        let marker = self.replies_marker(replies)?;
        self.check_reply_block(native, tip, block)?;
        self.check_reply_marker(native, block, &marker)?;
        if self.branch(native)?.1 != tip {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        Ok(())
    }

    pub fn read_published_replies(&self, native: &dyn ConsensusApi, block: Hash64) -> io::Result<PublishedPanelApproval> {
        let (_, tip) = self.branch(native)?;
        let approval = self.read_published_replies_on_branch(native, tip, block)?;
        if self.branch(native)?.1 != tip {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        Ok(approval)
    }

    fn check_reply_block(&self, native: &dyn ConsensusApi, tip: Hash64, block: Hash64) -> io::Result<()> {
        if native.get_header(block).map_err(io::Error::other)?.daa_score >= self.deadline {
            return Err(io::ErrorKind::TimedOut.into());
        }
        let parent = native.get_ghostdag_data(block).map_err(io::Error::other)?.selected_parent;
        if !native.is_chain_ancestor_of(self.certificate, parent).map_err(io::Error::other)?
            || !native.is_chain_ancestor_of(block, tip).map_err(io::Error::other)?
        {
            return Err(invalid("local PANEL replies outside committed branch"));
        }
        Ok(())
    }

    fn check_reply_marker(&self, native: &dyn ConsensusApi, block: Hash64, marker: &[u8; 64]) -> io::Result<()> {
        if !native.palw_coinbase_commits_to(self.chain, block, marker).map_err(io::Error::other)? {
            return Err(invalid("local PANEL replies outside committed branch"));
        }
        Ok(())
    }

    fn read_published_replies_on_branch(
        &self,
        native: &dyn ConsensusApi,
        tip: Hash64,
        block: Hash64,
    ) -> io::Result<PublishedPanelApproval> {
        self.replay_duty().verify_published(native, tip, block)
    }

    /// The reconstructed draw retained by the native certificate owner.
    pub fn replay_duty(&self) -> PanelReplayDuty {
        PanelReplayDuty {
            chain: self.chain,
            original: self.original,
            original_block: self.receipt,
            certificate: self.certificate,
            deadline: self.deadline,
            duty: self.duty,
            selected: self.selected.clone(),
        }
    }
}

impl LocalReplayDraw {
    /// Reconstruct expiration from the selected chain when every timely block
    /// has no possible private 64-byte receipt. An opaque commitment can be a
    /// signed reply set: withholding its opening must never establish absence.
    pub fn unverified_expiry_marker(&self, native: &dyn ConsensusApi, boundary: Hash64, scan_limit: usize) -> io::Result<[u8; 64]> {
        let (daa, tip) = self.branch(native)?;
        if daa < self.deadline {
            return Err(Tb1HistoryError::WindowNotOpen { daa, deadline: self.deadline }.into());
        }
        let parent = |block| native.get_ghostdag_data(block).map(|v| v.selected_parent).map_err(io::Error::other);
        let score = |block| native.get_header(block).map(|h| h.daa_score).map_err(io::Error::other);
        let mut cursor = parent(boundary)?;
        if !native.is_chain_ancestor_of(boundary, tip).map_err(io::Error::other)?
            || !native.is_chain_ancestor_of(self.certificate, cursor).map_err(io::Error::other)?
            || score(boundary)? < self.deadline
            || score(cursor)? >= self.deadline
        {
            return Err(invalid("wrong local PANEL deadline boundary"));
        }
        let mut budget = HistoryScanBudget::new("panel_deadline", tip, scan_limit);
        while cursor != self.certificate {
            budget.visit(cursor)?;
            if native.palw_coinbase_commitment(cursor).map_err(io::Error::other)?.is_some() {
                return Err(Tb1HistoryError::UnresolvedCommitment { block: cursor }.into());
            }
            cursor = parent(cursor)?;
        }
        if self.branch(native)?.1 != tip {
            return Err(Tb1HistoryError::ViewChanged { parent: tip }.into());
        }
        Ok(Sha3_512::new()
            .chain_update(b"MISAKA/private/TB1/PANEL/unverified-expiry/v1")
            .chain_update(self.duty)
            .chain_update(self.deadline.to_be_bytes())
            .chain_update(boundary.as_bytes())
            .finalize()
            .into())
    }

    /// Accept the negative terminal only on the same branch as the independently
    /// reconstructed boundary and the later block containing its marker.
    pub fn read_unverified_expiry(
        &self,
        native: &dyn ConsensusApi,
        boundary: Hash64,
        terminal: Hash64,
        scan_limit: usize,
    ) -> io::Result<()> {
        let tip = native.get_sink();
        let marker = self.unverified_expiry_marker(native, boundary, scan_limit)?;
        let parent = native.get_ghostdag_data(terminal).map_err(io::Error::other)?.selected_parent;
        if !native.is_chain_ancestor_of(boundary, parent).map_err(io::Error::other)?
            || !native.is_chain_ancestor_of(terminal, tip).map_err(io::Error::other)?
            || !native.palw_coinbase_commits_to(self.chain, terminal, &marker).map_err(io::Error::other)?
        {
            return Err(invalid("unverified expiry outside committed PANEL history"));
        }
        if self.branch(native)?.1 != tip {
            return Err(Tb1HistoryError::ViewChanged { parent: tip }.into());
        }
        Ok(())
    }
}
