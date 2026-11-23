//! Private model-free Original readback and the independent replay check.
//! The isolated envelope creates no entitlement or assessment result.
use super::*;
use super::{panel_cut::PublishedPanelApproval, terminal_case::BranchTerminalCourt};
use kaspa_consensus_core::{
    palw_artifact::PalwArtifactOpeningV1,
    palw_model_market_v1::moe2::{award_reference::Moe2AwardReference, settlement::Moe2ExecSlot},
    palw_qf1::{ReadoutMetadata, TreeKind, TreeRoot, WorkInput, readout_chunks::record_count, ticket},
    palw_state_v2::PalwChainStateV2,
    palw_tb1::qf1_original_carrier::{QF1_ORIGINAL_BODY_BYTES, Qf1OriginalCarrier},
    palw_tir_class_v1::PalwTirClassV1,
};
use misaka_palw_tir_exec::node::qf1::LocalTrace;
use serde::{Deserialize, Serialize};

pub const LOCAL_ORIGINAL_DOMAIN: &[u8] = b"MISAKA/private/QF1/local-original-signature/v1";

fn invalid_work(why: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, why)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, borsh::BorshSerialize, borsh::BorshDeserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalOriginalBody {
    pub counts: [u64; 4],
    pub roots: [Hash64; 4],
    pub ticket: [u8; 32],
}

pub(super) fn read_published_qf1_original(
    native: &dyn ConsensusApi,
    block: Hash64,
) -> io::Result<(LocalOriginalBody, MLDSA65Signature)> {
    let source = native.get_block(block).map_err(io::Error::other)?;
    if source.hash() != block {
        return Err(invalid("QF1 Original source block hash differs"));
    }
    decode_published_qf1_original(&source)
}

pub(super) fn decode_published_qf1_original(
    source: &kaspa_consensus_core::block::Block,
) -> io::Result<(LocalOriginalBody, MLDSA65Signature)> {
    let carrier = Qf1OriginalCarrier::from_block(source)
        .map_err(|why| io::Error::new(io::ErrorKind::InvalidData, format!("QF1 Original carrier: {why:?}")))?;
    Ok((borsh::from_slice(carrier.body)?, MLDSA65Signature::new(*carrier.signature)))
}

/// The envelope, target evaluation and numerical work identity on a
/// candidate branch. Independent replay and Gate C precede reward authority.
pub struct BranchQf1Execution {
    pub(super) close: Hash64,
    pub(super) registration: [u8; 64],
    pub(super) slot: u64,
    pub(super) quote: Option<Moe2PreDrawQuote>,
    parent: Hash64,
    original: Hash64,
    original_block: Hash64,
    work: Hash64,
    attempt: [u8; 32],
    ticket: [u8; 32],
    ticket_meets_target: bool,
}

/// A target-meeting execution and its complete replay approval on the same
/// selected parent. Gate C certification and mining eligibility are separate.
pub struct PanelApprovedQf1Execution {
    execution: BranchQf1Execution,
    approval: PublishedPanelApproval,
}

impl PanelApprovedQf1Execution {
    pub fn execution(&self) -> &BranchQf1Execution {
        &self.execution
    }

    pub fn approval(&self) -> PublishedPanelApproval {
        self.approval
    }

    /// Match the terminal Court read, signed quote and replay indexes against
    /// the selected parent's reservation. Gate C and pricing have their own reads.
    pub fn bind_moe2_funding(
        &self,
        native: &dyn ConsensusApi,
        reference: &Moe2AwardReference,
        parent: &PalwChainStateV2,
        terminal_court: &BranchTerminalCourt,
        daa: u64,
    ) -> io::Result<Moe2ExecSlot> {
        let execution = &self.execution;
        let point = parent.last_point().ok_or_else(|| invalid("MOE2 award has no selected parent state"))?;
        if point.block != execution.parent || point.daa_score > daa {
            return Err(invalid("MOE2 award funding belongs to a different selected parent"));
        }
        if (terminal_court.parent(), terminal_court.original()) != (execution.parent, execution.original) {
            return Err(invalid("terminal Court belongs to a different selected parent or Original"));
        }
        if terminal_court.blocks_success() {
            return Err(invalid("terminal Court does not permit this MOE2 award"));
        }
        let quote = execution.quote.ok_or_else(|| invalid("approved EXEC input carries no signed MOE2 quote"))?;
        let close = native.get_header(execution.close).map_err(io::Error::other)?;
        if close.hash != execution.close {
            return Err(invalid("MOE2 EXEC close hash differs from the approved execution"));
        }
        let slot = Moe2ExecSlot {
            registration: execution.registration,
            slot: execution.slot,
            close: execution.close,
            close_daa: close.daa_score,
            quote,
        };
        reference.check_execution_slot(execution.original, execution.attempt, &slot).map_err(invalid)?;
        reference.check_parent_reservation(parent, daa).map_err(invalid)?;
        reference.check_parent_unspent(parent).map_err(invalid)?;
        if parent.private_moe2_work_claim(&execution.work).is_some() {
            return Err(invalid("private MOE2 work has already settled on this branch"));
        }
        Ok(slot)
    }
}

impl BranchQf1Execution {
    pub fn parent(&self) -> Hash64 {
        self.parent
    }

    pub fn original(&self) -> Hash64 {
        self.original
    }

    pub fn original_block(&self) -> Hash64 {
        self.original_block
    }

    pub fn attempt(&self) -> &[u8; 32] {
        &self.attempt
    }

    pub fn work(&self) -> Hash64 {
        self.work
    }

    pub fn ticket(&self) -> &[u8; 32] {
        &self.ticket
    }

    pub fn ticket_meets_target(&self) -> bool {
        self.ticket_meets_target
    }
}

impl BranchExecInput<'_> {
    /// Read the Original named by this branch's verified Panel approval.
    pub fn read_panel_approved_qf1_execution(
        &self,
        native: &dyn ConsensusApi,
        approval: PublishedPanelApproval,
        instance: &misaka_palw_tir_exec::node::qf1::LocalInstance<'_>,
    ) -> io::Result<PanelApprovedQf1Execution> {
        self.read_approved_qf1(native, approval, |input| input.local_work_key(instance).map_err(invalid_work))
    }

    /// Recover an approved Original using the signed class and Readout
    /// preimages and a single ordinary artifact opening. No weight file or
    /// model execution is needed for this work-identity read.
    pub fn read_panel_approved_qf1_execution_with_metadata(
        &self,
        native: &dyn ConsensusApi,
        approval: PublishedPanelApproval,
        class: &PalwTirClassV1,
        readout: &ReadoutMetadata,
        opening: &PalwArtifactOpeningV1,
    ) -> io::Result<PanelApprovedQf1Execution> {
        self.read_approved_qf1(native, approval, |input| {
            let terms = input.terms;
            let frame = terms.qf1.ok_or_else(|| invalid("signed EXEC input has no QF1 frame"))?.admitted()?;
            let model = readout
                .open_work_model(class, terms.model_root, terms.class_id, terms.tokenizer, frame.instance_descriptor, opening)
                .map_err(invalid_work)?;
            let work_input = WorkInput::new(&input.tokens, terms.decode_tokens).map_err(invalid_work)?;
            Ok(model.work_key(work_input))
        })
    }

    fn read_approved_qf1(
        &self,
        native: &dyn ConsensusApi,
        approval: PublishedPanelApproval,
        work: impl FnOnce(&DerivedExecInput<'_>) -> io::Result<Hash64>,
    ) -> io::Result<PanelApprovedQf1Execution> {
        if approval.selected_parent() != self.parent {
            return Err(invalid("PANEL approval belongs to a different selected parent"));
        }
        let execution = self.read_published_qf1_execution(native, approval.original_block(), approval.original(), work)?;
        if !execution.ticket_meets_target() {
            return Err(invalid("PANEL Original does not meet its signed EXEC target"));
        }
        Ok(PanelApprovedQf1Execution { execution, approval })
    }

    /// Recover the owner-signed Original from the source block on this branch.
    /// The award's expected claim must equal the Original derived from EXEC.
    pub fn read_qf1_execution_from_block(
        &self,
        native: &dyn ConsensusApi,
        block: Hash64,
        expected_original: Hash64,
        instance: &misaka_palw_tir_exec::node::qf1::LocalInstance<'_>,
    ) -> io::Result<BranchQf1Execution> {
        self.read_published_qf1_execution(native, block, expected_original, |input| {
            input.local_work_key(instance).map_err(invalid_work)
        })
    }

    fn read_published_qf1_execution(
        &self,
        native: &dyn ConsensusApi,
        block: Hash64,
        expected_original: Hash64,
        work: impl FnOnce(&DerivedExecInput<'_>) -> io::Result<Hash64>,
    ) -> io::Result<BranchQf1Execution> {
        let (body, signature) = read_published_qf1_original(native, block)?;
        if body.signing_id(&self.input) != expected_original {
            return Err(invalid("QF1 Original differs from the expected claim"));
        }
        self.read_qf1_execution_with_work(native, block, &body, &signature, work)
    }

    /// Bind a recorded Original and its numerical work. This reads the
    /// envelope and the candidate branch's commitment without model replay.
    pub fn read_qf1_execution(
        &self,
        native: &dyn ConsensusApi,
        block: Hash64,
        body: &LocalOriginalBody,
        sig: &MLDSA65Signature,
        instance: &misaka_palw_tir_exec::node::qf1::LocalInstance<'_>,
    ) -> io::Result<BranchQf1Execution> {
        self.read_qf1_execution_with_work(native, block, body, sig, |input| input.local_work_key(instance).map_err(invalid_work))
    }

    fn read_qf1_execution_with_work(
        &self,
        native: &dyn ConsensusApi,
        block: Hash64,
        body: &LocalOriginalBody,
        sig: &MLDSA65Signature,
        work: impl FnOnce(&DerivedExecInput<'_>) -> io::Result<Hash64>,
    ) -> io::Result<BranchQf1Execution> {
        let input = &self.input;
        let original = input.read_qf1_original_on_branch(native, self.parent, block, body, sig)?;
        let work = work(input)?;
        Ok(BranchQf1Execution {
            close: input.close,
            registration: input.registration_key,
            slot: input.slot,
            quote: input.terms.moe2_quote,
            parent: self.parent,
            original,
            original_block: block,
            work,
            attempt: *input.attempt(),
            ticket: body.ticket,
            ticket_meets_target: input.terms.threshold()?.admits_ticket_be(&body.ticket),
        })
    }
}

impl LocalOriginalBody {
    /// Encode the body and its existing owner signature for a lifecycle
    /// transaction. The header's Original ID has its existing meaning.
    pub fn publication_payload(&self, signature: &MLDSA65Signature) -> Vec<u8> {
        let mut body = [0; QF1_ORIGINAL_BODY_BYTES];
        borsh::BorshSerialize::serialize(self, &mut body.as_mut_slice()).expect("fixed-size QF1 Original body");
        Qf1OriginalCarrier { body: &body, signature: signature.as_slice().try_into().expect("fixed-size ML-DSA-65 signature") }
            .encode()
    }

    pub(crate) fn from_trace(trace: &LocalTrace) -> Self {
        Self { counts: trace.roots.map(|r| r.count()), roots: trace.roots.map(|r| Hash64::from_bytes(r.digest)), ticket: trace.ticket }
    }

    /// Canonical signature message for the received fields and this EXEC instance.
    pub fn signing_id(&self, input: &DerivedExecInput<'_>) -> Hash64 {
        let mut body = [0; QF1_ORIGINAL_BODY_BYTES];
        borsh::BorshSerialize::serialize(self, &mut body.as_mut_slice()).expect("fixed-size QF1 Original body");
        kaspa_consensus_core::palw_tb1::qf1_original_carrier::signing_id(
            &input.chain,
            &input.registration_key,
            input.slot,
            &input.attempt,
            &body,
        )
    }
}

/// The body produced by numerical execution and the optional disclosure are
/// immutable together. Signature randomness and disclosure coordinates do not enter the ID.
pub struct LocalOriginal<'i, 't> {
    pub(super) input: &'i DerivedExecInput<'t>,
    pub(super) trace: LocalTrace,
}

impl LocalOriginal<'_, '_> {
    pub fn trace(&self) -> &LocalTrace {
        &self.trace
    }
    pub fn body(&self) -> LocalOriginalBody {
        LocalOriginalBody::from_trace(&self.trace)
    }
    /// Numerical comparison with the target signed before EXEC release.
    /// Local inference carries no entitlement, including on a matching ticket.
    pub fn ticket_meets_target(&self) -> io::Result<bool> {
        Ok(self.input.terms.threshold()?.admits_ticket_be(&self.trace.ticket))
    }
    pub fn id(&self) -> Hash64 {
        self.body().signing_id(self.input)
    }

    /// Verify the received owner's body by complete numerical replay.
    pub fn verify(&self, native: &dyn ConsensusApi, body: &LocalOriginalBody, sig: &MLDSA65Signature) -> io::Result<()> {
        if body != &self.body() {
            return Err(invalid("different replayed local QF1 Original"));
        }
        self.input.verify_qf1_body(native, body, sig)?;
        Ok(())
    }
}

impl DerivedExecInput<'_> {
    /// Bounded envelope checks only. X comes from the selected EXEC history.
    /// The signature commits Y/Z/TRACE; their numerical origin is checked by replay.
    pub fn verify_qf1_body(&self, native: &dyn ConsensusApi, body: &LocalOriginalBody, sig: &MLDSA65Signature) -> io::Result<Hash64> {
        self.check_live(native)?;
        let id = self.verify_original_body(body, sig)?;
        self.check_live(native)?;
        Ok(id)
    }

    pub(super) fn verify_original_body(&self, body: &LocalOriginalBody, sig: &MLDSA65Signature) -> io::Result<Hash64> {
        self.terms.qf1.ok_or_else(|| invalid("registration has no QF1 frame"))?.admitted()?;
        let [x, t, z, records] = body.counts;
        if x != self.tokens.len() as u64
            || !(1..=u64::from(self.terms.decode_tokens)).contains(&t)
            || z != 1
            || records > self.terms.max_leaves
            || records < t + u64::from(record_count(t as u32).map_err(invalid)?) + 3
        {
            return Err(invalid("local QF1 Original counts outside registration"));
        }
        let claimed = |kind, idx: usize| {
            TreeRoot::from_claimed_digest(self.attempt, kind, body.counts[idx], body.roots[idx].as_bytes()).map_err(invalid)
        };
        let mut x_tree =
            kaspa_consensus_core::palw_qf1::TreeBuilder::new(self.attempt, TreeKind::Input, x, x, None).map_err(invalid)?;
        for id in &self.tokens {
            x_tree.push(&id.to_le_bytes()).map_err(invalid)?;
        }
        let rx = x_tree.finish().map_err(invalid)?.0;
        if rx.digest != body.roots[0].as_bytes() {
            return Err(invalid("local Original differs from derived EXEC input"));
        }
        let roots = [rx, claimed(TreeKind::Answer, 1)?, claimed(TreeKind::Readout, 2)?, claimed(TreeKind::Trace, 3)?];
        if ticket(roots.each_ref()).map_err(invalid)? != body.ticket {
            return Err(invalid("local Original ticket mismatch"));
        }
        let original = body.signing_id(self);
        ml_dsa_65::verify(self.owner, original.as_byte_slice(), LOCAL_ORIGINAL_DOMAIN, sig)
            .map_err(|_| invalid("invalid local Original owner signature"))?;
        Ok(original)
    }

    /// Read the private receipt from the actual selected block after EXEC
    /// publication. An off-chain signed packet alone cannot open the PANEL cut.
    pub fn read_qf1_original(
        &self,
        native: &dyn ConsensusApi,
        block: Hash64,
        body: &LocalOriginalBody,
        sig: &MLDSA65Signature,
    ) -> io::Result<Hash64> {
        self.check_live(native)?;
        let id = self.read_recorded_qf1_original(native, block, body, sig)?;
        self.check_live(native)?;
        Ok(id)
    }

    pub(super) fn read_recorded_qf1_original(
        &self,
        native: &dyn ConsensusApi,
        block: Hash64,
        body: &LocalOriginalBody,
        sig: &MLDSA65Signature,
    ) -> io::Result<Hash64> {
        let (_, tip) = self.check_selected(native)?;
        let original = self.read_qf1_original_on_branch(native, tip, block, body, sig)?;
        if self.check_selected(native)?.1 != tip {
            return Err(io::Error::new(io::ErrorKind::WouldBlock, "local Original view moved"));
        }
        Ok(original)
    }

    pub(super) fn read_qf1_original_on_branch(
        &self,
        native: &dyn ConsensusApi,
        selected_parent: Hash64,
        block: Hash64,
        body: &LocalOriginalBody,
        sig: &MLDSA65Signature,
    ) -> io::Result<Hash64> {
        let original = self.verify_original_body(body, sig)?;
        let header = native.get_header(block).map_err(io::Error::other)?;
        let parent = native.get_ghostdag_data(block).map_err(io::Error::other)?.selected_parent;
        if !(self.terms.open_daa..self.terms.deadline_exclusive_daa).contains(&header.daa_score)
            || !native.is_chain_ancestor_of(self.certificate, parent).map_err(io::Error::other)?
            || !native.is_chain_ancestor_of(block, selected_parent).map_err(io::Error::other)?
            || !native.palw_coinbase_commits_to(self.chain, block, &original.as_bytes()).map_err(io::Error::other)?
        {
            return Err(invalid("local Original outside committed EXEC history"));
        }
        Ok(original)
    }
}
