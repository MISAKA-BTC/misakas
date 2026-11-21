//! Private model-free Original readback and the independent replay check.
//! The envelope is local-only and creates no entitlement or assessment result.
use super::*;
use kaspa_consensus_core::palw_qf1::{TreeKind, TreeRoot, readout_chunks::record_count, ticket};
use misaka_palw_tir_exec::node::qf1::LocalTrace;
use serde::{Deserialize, Serialize};

pub const LOCAL_ORIGINAL_DOMAIN: &[u8] = b"MISAKA/private/QF1/local-original-signature/v1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, borsh::BorshSerialize)]
#[serde(deny_unknown_fields)]
pub struct LocalOriginalBody {
    pub counts: [u64; 4],
    pub roots: [Hash64; 4],
    pub ticket: [u8; 32],
}

impl LocalOriginalBody {
    pub(crate) fn from_trace(trace: &LocalTrace) -> Self {
        Self { counts: trace.roots.map(|r| r.count()), roots: trace.roots.map(|r| Hash64::from_bytes(r.digest)), ticket: trace.ticket }
    }

    /// Canonical signature message for the received fields and this EXEC instance.
    pub fn signing_id(&self, input: &DerivedExecInput<'_>) -> Hash64 {
        let mut hash = Sha3_512::new()
            .chain_update(b"MISAKA/private/QF1/local-original/v1")
            .chain_update(input.chain)
            .chain_update(input.registration_key)
            .chain_update(input.slot.to_be_bytes())
            .chain_update(input.attempt);
        for (&n, h) in self.counts.iter().zip(&self.roots) {
            hash.update(n.to_le_bytes());
            hash.update(h.as_byte_slice());
        }
        hash.update(self.ticket);
        Hash64::from_bytes(hash.finalize().into())
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
        self.check_live(native)?;
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
        let tip = native.get_sink();
        let original = self.verify_qf1_body(native, body, sig)?;
        let header = native.get_header(block).map_err(io::Error::other)?;
        let parent = native.get_ghostdag_data(block).map_err(io::Error::other)?.selected_parent;
        if !(self.terms.open_daa..self.terms.deadline_exclusive_daa).contains(&header.daa_score)
            || !native.is_chain_ancestor_of(self.certificate, parent).map_err(io::Error::other)?
            || !native.is_chain_ancestor_of(block, tip).map_err(io::Error::other)?
            || !native.palw_coinbase_commits_to(self.chain, block, &original.as_bytes()).map_err(io::Error::other)?
        {
            return Err(invalid("local Original outside committed EXEC history"));
        }
        if native.get_sink() != tip {
            return Err(io::Error::new(io::ErrorKind::WouldBlock, "local Original view moved"));
        }
        Ok(original)
    }
}
