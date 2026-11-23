//! Private PANEL consumers. Candidates and complete-replay duties come from
//! the earlier native close. The dedicated certificate is the only draw input
//! that becomes known after that close. No positive vote grants mining credit.
use super::{original::read_published_qf1_original, *};
use kaspa_consensus_core::palw_panel_v2::{
    PALW_DRAW_WEIGHT_MAX_MSK_V1, palw_draw_key_cmp_v1, palw_draw_neg_log2_q64_v1, palw_draw_ticket_u64_v1, palw_panel_stake_ticket_v1,
};
use misaka_palw_tir_exec::node::qf1::LocalInstance;

pub const REPLAY_SIGNATURE_DOMAIN: &[u8] = b"MISAKA/private/TB1/QF1/complete-replay/v1";

pub fn actor_id(key: &MLDSA65VerificationKey) -> Hash64 {
    Hash64::from_bytes(
        Sha3_512::new().chain_update(b"MISAKA/private/TB1/PANEL/actor/v1").chain_update(key.as_slice()).finalize().into(),
    )
}

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
            let mut key = [0; 1952];
            let mut signature = [0; 3309];
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
        Hash64::from_bytes(
            Sha3_512::new()
                .chain_update(b"MISAKA/private/TB1/PANEL/stake-ticket/full-replay/v1")
                .chain_update(self.seats.to_be_bytes())
                .finalize()
                .into(),
        )
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
        let draw = self.reconstruct_draw(native, close, gate, batch, certificate, rows, q, limits, original, policy, freeze)?;
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
        let draw = self.reconstruct_draw(native, close, gate, batch, certificate, rows, q, limits, original, policy, freeze)?;
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

    #[allow(clippy::too_many_arguments)]
    fn reconstruct_draw(
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
        freeze: Tb1HolderFreeze,
    ) -> io::Result<LocalReplayDraw> {
        let seats = policy.seats as usize;
        let receipt = self.receipts.iter().find(|r| r.original == original).ok_or_else(|| invalid("Original outside PANEL cut"))?;
        if !receipt.meets_target
            || policy.id().as_bytes() != self.closure.policy_root
            || seats > self.candidates.len()
            || self.candidates.iter().any(|(_, w)| *w > PALW_DRAW_WEIGHT_MAX_MSK_V1)
            || gate.holder_freeze().accepting_block != close
            || gate.holder_freeze().freeze_id != freeze.freeze_id
        {
            return Err(invalid("different PANEL policy, population or publication gate"));
        }
        let seed = gate.read_seed(native, batch, certificate, rows, q, limits)?;
        let anchor = Hash64::from_bytes(
            Sha3_512::new()
                .chain_update(b"MISAKA/private/TB1/PANEL/draw-anchor/v1")
                .chain_update(freeze.freeze_id)
                .chain_update(seed.as_bytes())
                .finalize()
                .into(),
        );
        let mut ranked: Vec<_> = self
            .candidates
            .iter()
            .map(|(id, w)| {
                let ticket = palw_panel_stake_ticket_v1(anchor, &original, id);
                (palw_draw_neg_log2_q64_v1(palw_draw_ticket_u64_v1(&ticket)), *w, *id)
            })
            .collect();
        ranked.sort_unstable_by(|a, b| palw_draw_key_cmp_v1(a.0, a.1, &a.2, b.0, b.1, &b.2));
        let selected = ranked.into_iter().take(seats).map(|(_, _, id)| id).collect();
        let duty = Sha3_512::new()
            .chain_update(REPLAY_SIGNATURE_DOMAIN)
            .chain_update(freeze.freeze_id)
            .chain_update(original.as_bytes())
            .chain_update(receipt.block.as_bytes())
            .chain_update(certificate.as_bytes())
            .chain_update(anchor.as_bytes())
            .finalize()
            .into();
        Ok(LocalReplayDraw {
            chain: self.closure.round.chain_id,
            original,
            receipt: receipt.block,
            close,
            certificate,
            deadline: receipt.deadline,
            selected,
            duty,
        })
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
        Ok(Sha3_512::new().chain_update(self.duty).chain_update(actor.as_bytes()).finalize().into())
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

    fn replies_marker(&self, replies: &[(&MLDSA65VerificationKey, &MLDSA65Signature)]) -> io::Result<[u8; 64]> {
        let mut seats = self.selected.iter();
        let mut h = Sha3_512::new().chain_update(b"MISAKA/private/TB1/PANEL/reply-set/v1").chain_update(self.duty);
        for &(key, sig) in replies {
            let id = actor_id(key);
            if !seats.any(|seat| *seat == id) {
                return Err(invalid("PANEL replies outside draw order"));
            }
            let msg = self.message(key)?;
            ml_dsa_65::verify(key, &msg, REPLAY_SIGNATURE_DOMAIN, sig).map_err(|_| invalid("invalid local PANEL reply"))?;
            h.update(id.as_bytes());
            h.update(sig.as_slice());
        }
        if replies.len() != self.selected.len() {
            return Err(io::Error::new(io::ErrorKind::WouldBlock, "incomplete local PANEL replies"));
        }
        Ok(h.finalize().into())
    }
    pub fn read_replies(
        &self,
        native: &dyn ConsensusApi,
        block: Hash64,
        replies: &[(&MLDSA65VerificationKey, &MLDSA65Signature)],
    ) -> io::Result<()> {
        let (_, tip) = self.branch(native)?;
        let marker = self.replies_marker(replies)?;
        if native.get_header(block).map_err(io::Error::other)?.daa_score >= self.deadline {
            return Err(io::ErrorKind::TimedOut.into());
        }
        let parent = native.get_ghostdag_data(block).map_err(io::Error::other)?.selected_parent;
        if !native.is_chain_ancestor_of(self.certificate, parent).map_err(io::Error::other)?
            || !native.is_chain_ancestor_of(block, tip).map_err(io::Error::other)?
            || !native.palw_coinbase_commits_to(self.chain, block, &marker).map_err(io::Error::other)?
        {
            return Err(invalid("local PANEL replies outside committed branch"));
        }
        if self.branch(native)?.1 != tip {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        Ok(())
    }
}

impl LocalReplayDraw {
    /// Reconstruct expiration from the selected chain when every timely block
    /// has no possible private 64-byte receipt. An opaque commitment can be a
    /// signed reply set: withholding its opening must never establish absence.
    pub fn unverified_expiry_marker(&self, native: &dyn ConsensusApi, boundary: Hash64, scan_limit: usize) -> io::Result<[u8; 64]> {
        let (daa, tip) = self.branch(native)?;
        if daa < self.deadline {
            return Err(io::ErrorKind::WouldBlock.into());
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
        let mut remaining = scan_limit;
        while cursor != self.certificate {
            remaining =
                remaining.checked_sub(1).ok_or_else(|| io::Error::new(io::ErrorKind::WouldBlock, "deadline scan budget exhausted"))?;
            if native.palw_coinbase_commitment(cursor).map_err(io::Error::other)?.is_some() {
                return Err(io::Error::new(io::ErrorKind::WouldBlock, format!("unresolved timely PANEL commitment at {cursor}")));
            }
            cursor = parent(cursor)?;
        }
        if self.branch(native)?.1 != tip {
            return Err(io::ErrorKind::WouldBlock.into());
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
            return Err(io::ErrorKind::WouldBlock.into());
        }
        Ok(())
    }
}
