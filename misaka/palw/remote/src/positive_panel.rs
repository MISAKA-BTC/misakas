//! r1 Panel signature/transport adapter. Uses the A0 contract directly. A local
//! batch authenticates its destination and ordering, never arithmetic or credit.
use crate::bundle::DetachedSigner;
use misaka_palw_positive_contract::{
    ContractError,
    identity::{ChainScope, EVIDENCE_SIGN_CONTEXT, PUBKEY_BYTES, SIGNATURE_BYTES, WITNESS_SIGN_CONTEXT},
    messages::{
        AuthenticatedResult, CheckProfile, CoverageCursor, CoverageDescriptor, MinerResultCommit, PanelWitnessCommit, coverage_plan_id,
    },
    ports::{NodeChecker, require_next},
    tir,
    wire::{EvidenceFrame, encode_witness},
};

#[derive(Debug)]
pub enum PanelTransportError<E> {
    Contract(ContractError),
    Node(E),
}

/// Preserve this exact signed witness across relays/restarts. Key resolution
/// and rooted admission remain at A4, independently of a candidate's contents.
pub struct SignedPanelWitnessR1 {
    result: MinerResultCommit,
    plan: CoverageDescriptor,
    witness: PanelWitnessCommit,
    signature: [u8; SIGNATURE_BYTES],
    author_key: [u8; PUBKEY_BYTES],
}
impl SignedPanelWitnessR1 {
    pub fn sign(
        result: &AuthenticatedResult<'_>,
        plan: CoverageDescriptor,
        witness: PanelWitnessCommit,
        signer: &dyn DetachedSigner,
    ) -> Result<Self, String> {
        Self::check_binding(result.commit(), &plan, &witness).map_err(|e| format!("Panel r1 binding: {e:?}"))?;
        let key: [u8; PUBKEY_BYTES] = signer.public_key().try_into().map_err(|_| "Panel r1 public key size")?;
        if misaka_palw_positive_contract::identity::principal(&key) != witness.author {
            return Err("Panel r1 signer is not witness author".into());
        }
        let signature = signer.sign_with_context(&witness.id().0.0, WITNESS_SIGN_CONTEXT)?;
        let signature: [u8; SIGNATURE_BYTES] = signature.try_into().map_err(|_| "Panel r1 signature size")?;
        Self::from_signed(result, plan, witness, key, signature).map_err(|e| format!("Panel r1 signature: {e:?}"))
    }

    /// Reauthenticate the original author. Replacement transport identities do
    /// not rewrite the witness/attempt or buy a second service allowance.
    pub fn from_signed(
        result: &AuthenticatedResult<'_>,
        plan: CoverageDescriptor,
        witness: PanelWitnessCommit,
        author_key: [u8; PUBKEY_BYTES],
        signature: [u8; SIGNATURE_BYTES],
    ) -> Result<Self, ContractError> {
        Self::check_binding(result.commit(), &plan, &witness)?;
        witness.authenticate(result.commit(), &author_key, &signature)?;
        Ok(Self { result: result.commit().clone(), plan, witness, signature, author_key })
    }

    fn check_binding(
        result: &MinerResultCommit,
        plan: &CoverageDescriptor,
        witness: &PanelWitnessCommit,
    ) -> Result<(), ContractError> {
        result.validate_shape()?;
        witness.validate_binding(result)?;
        encode_witness(witness)?;
        if result.selector != tir::greedy_selector_id() || result.temperature_q24 != 0 || plan.profile != CheckProfile::ExactStreamV1 {
            return Err(ContractError::UnsupportedProfile);
        }
        if plan.scope != result.scope
            || plan.result != result.id()
            || witness.plan != plan.plan
            || witness.profile != plan.profile
            || witness.provenance != tir::provenance_id(result)
            || plan.plan != coverage_plan_id(result, plan.profile, tir::exact_derivation_id(), plan.mandatory_relations)?
        {
            return Err(ContractError::Identity);
        }
        tir::raw_part_budget(plan.max_part_bytes)?;
        Ok(())
    }
    pub fn witness(&self) -> &PanelWitnessCommit {
        &self.witness
    }
    pub fn signature(&self) -> &[u8; SIGNATURE_BYTES] {
        &self.signature
    }
    pub fn author_key(&self) -> &[u8; PUBKEY_BYTES] {
        &self.author_key
    }
    pub fn plan(&self) -> CoverageDescriptor {
        self.plan
    }
    pub fn result(&self) -> &MinerResultCommit {
        &self.result
    }

    /// Authenticate exact common frame bytes under the original author key.
    /// Relays keep this packet unchanged; a delivery is not a Node verdict.
    pub fn sign_part(&self, frame: &EvidenceFrame<'_>, signer: &dyn DetachedSigner) -> Result<SignedPanelPartR1, String> {
        if signer.public_key().as_slice() != self.author_key {
            return Err("Panel frame signer is not the original witness author".into());
        }
        require_next(
            &self.plan,
            CoverageCursor { plan: self.plan.plan, next_relation: frame.header().first_relation },
            &self.witness,
            frame,
        )
        .map_err(|e| format!("Panel frame identity: {e:?}"))?;
        tir::decode_frame(frame, self.plan.max_part_bytes).map_err(|e| format!("Panel frame payload: {e:?}"))?;
        let signature: [u8; SIGNATURE_BYTES] =
            signer.sign_with_context(&frame.id().0.0, EVIDENCE_SIGN_CONTEXT)?.try_into().map_err(|_| "Panel frame signature size")?;
        let cap = self.plan.max_part_bytes.checked_add(SIGNATURE_BYTES as u64).ok_or("Panel signed frame cap overflow")?;
        frame.authenticate(&self.witness, &self.author_key, &signature, cap).map_err(|e| format!("Panel frame signature: {e:?}"))?;
        let bytes = frame.encode(self.plan.max_part_bytes).map_err(|e| format!("Panel frame encoding: {e:?}"))?;
        SignedPanelPartR1::from_signed(bytes, signature, self.plan.max_part_bytes).map_err(|e| format!("Panel signed part: {e:?}"))
    }

    /// `cursor` must be read from the receiving Node's live/replayed progress.
    /// Supplying a local send cursor cannot advance the Node or finish coverage.
    pub fn next_range(&self, view: ChainScope, cursor: CoverageCursor, max_parts: u64) -> Result<(u64, u64), ContractError> {
        if view != self.result.scope || cursor.plan != self.plan.plan {
            return Err(ContractError::Scope);
        }
        if max_parts == 0 {
            return Err(ContractError::Budget);
        }
        let remaining = self.plan.mandatory_relations.checked_sub(cursor.next_relation).ok_or(ContractError::Range)?;
        Ok((cursor.next_relation, remaining.min(max_parts)))
    }

    pub fn batch(&self, first: u64, count: u64, max_parts: u64, max_bytes: u64) -> Result<PanelBatchR1, ContractError> {
        if count == 0
            || count > max_parts
            || first.checked_add(count).filter(|n| *n <= self.plan.mandatory_relations).is_none()
            || count > max_bytes / std::mem::size_of::<SignedPanelPartR1>() as u64
        {
            return Err(ContractError::Budget);
        }
        Ok(PanelBatchR1 {
            plan: self.plan,
            witness: self.witness.clone(),
            author_key: self.author_key,
            first,
            count,
            max_bytes,
            bytes: 0,
            frames: Vec::new(),
        })
    }
}

/// Common frame plus its detached A0 signature. This process DTO defines no
/// native wire tag. Construction checks framing; intake checks the admitted
/// ORIGINAL author's signature before touching any arithmetic-check progress.
pub struct SignedPanelPartR1 {
    bytes: Vec<u8>,
    signature: [u8; SIGNATURE_BYTES],
}
impl SignedPanelPartR1 {
    pub fn from_signed(bytes: Vec<u8>, signature: [u8; SIGNATURE_BYTES], max_frame_bytes: u64) -> Result<Self, ContractError> {
        EvidenceFrame::decode(&bytes, max_frame_bytes)?;
        Ok(Self { bytes: bytes.to_vec(), signature })
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn signature(&self) -> &[u8; SIGNATURE_BYTES] {
        &self.signature
    }
}

/// A bounded set of formal r1 frames, retaining accepted-prefix semantics when
/// a receiving Node rejects a later relation. No persisted local pass/cursor.
pub struct PanelBatchR1 {
    plan: CoverageDescriptor,
    witness: PanelWitnessCommit,
    author_key: [u8; PUBKEY_BYTES],
    first: u64,
    count: u64,
    max_bytes: u64,
    bytes: u64,
    frames: Vec<SignedPanelPartR1>,
}
impl PanelBatchR1 {
    pub fn push(&mut self, packet: &SignedPanelPartR1) -> Result<(), ContractError> {
        if self.frames.len() as u64 >= self.count {
            return Err(ContractError::Range);
        }
        let frame = EvidenceFrame::decode(&packet.bytes, self.plan.max_part_bytes)?;
        let signed_cap = self.plan.max_part_bytes.checked_add(SIGNATURE_BYTES as u64).ok_or(ContractError::Budget)?;
        frame.authenticate(&self.witness, &self.author_key, &packet.signature, signed_cap)?;
        let cursor = CoverageCursor { plan: self.plan.plan, next_relation: self.first + self.frames.len() as u64 };
        let next = require_next(&self.plan, cursor, &self.witness, &frame)?;
        if next != cursor.next_relation + 1 {
            return Err(ContractError::NonCanonical);
        }
        tir::decode_frame(&frame, self.plan.max_part_bytes)?;
        let total = self.bytes.checked_add(frame.signed_wire_bytes()).filter(|n| *n <= self.max_bytes).ok_or(ContractError::Budget)?;
        self.frames.push(SignedPanelPartR1 { bytes: packet.bytes.to_vec(), signature: packet.signature });
        self.bytes = total;
        Ok(())
    }
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
    pub fn check_with<N: NodeChecker>(
        &self,
        checker: &N,
        admitted: &N::Context,
        progress: &mut N::Progress,
    ) -> Result<u64, PanelTransportError<N::Error>> {
        if self.frames.len() as u64 != self.count {
            return Err(PanelTransportError::Contract(ContractError::NonCanonical));
        }
        let node_plan = checker.derive_plan(admitted).map_err(PanelTransportError::Node)?;
        if node_plan != self.plan {
            return Err(PanelTransportError::Contract(ContractError::Identity));
        }
        for packet in &self.frames {
            let frame = EvidenceFrame::decode(&packet.bytes, self.plan.max_part_bytes).map_err(PanelTransportError::Contract)?;
            let signed_cap = self
                .plan
                .max_part_bytes
                .checked_add(SIGNATURE_BYTES as u64)
                .ok_or(PanelTransportError::Contract(ContractError::Budget))?;
            let authenticated = frame
                .authenticate(&self.witness, &self.author_key, &packet.signature, signed_cap)
                .map_err(PanelTransportError::Contract)?;
            checker.check_part(admitted, progress, authenticated.frame()).map_err(PanelTransportError::Node)?;
        }
        Ok(self.count)
    }
}
