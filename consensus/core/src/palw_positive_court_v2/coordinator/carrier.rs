//! A4's concrete Panel-signature carrier over A0's private interchange helper.
//!
//! The signature authenticates these exact evidence bytes before A1 sees them;
//! it never certifies computation. Relays can retransmit the identical signed
//! packet without changing its Panel author. No native transaction tag, native
//! transaction overhead, ingress admission, or activation is allocated here.
use super::contract::CarrierVerifierV2;
use misaka_palw_positive_contract::{
    ContractError,
    identity::{PUBKEY_BYTES, SIGNATURE_BYTES},
    messages::{MinerResultCommit, PanelWitnessCommit},
    wire::{EVIDENCE_HEADER_BYTES, EvidenceFrame},
};

/// Process input, not a newly allocated wire schema. The public key must be
/// resolved/matched against the already registered rooted Panel identity. The
/// private interchange counts the signature (4627 bytes), assuming that key is
/// resolved from state. A transport carrying this whole key array on the wire
/// must ALSO charge PUBKEY_BYTES; a native transaction must add its own overhead.
pub struct PanelPacketSignatureV2 {
    pub key: [u8; PUBKEY_BYTES],
    pub signature: [u8; SIGNATURE_BYTES],
}

/// Resolve the live authorized prover role, opportunity, author, reserved
/// resources and signed-packet limit. This is NOT the Gate C draw.
/// Implementations MUST check the actual rooted author/key registration, duty,
/// current network/ruleset/branch and clock against result/witness. The returned
/// finite limit includes the whole unsigned frame plus its Panel signature.
/// It is not the witness's self-reported size or permission. There is no
/// permissive default authority or fallback to an asserted caller boolean.
pub trait PanelFrameAuthorityV2 {
    type Error;
    fn authorize(&self, result: &MinerResultCommit, witness: &PanelWitnessCommit) -> Result<u64, Self::Error>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UnassignedPanelAuthorityV2;
impl PanelFrameAuthorityV2 for UnassignedPanelAuthorityV2 {
    type Error = ContractError;
    fn authorize(&self, _: &MinerResultCommit, _: &PanelWitnessCommit) -> Result<u64, Self::Error> {
        Err(ContractError::NativeAdapterUnassigned)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum PanelCarrierErrorV2<AuthorityError> {
    Authority(AuthorityError),
    Contract(ContractError),
}

pub struct PanelSignatureVerifierV2<R> {
    authority: R,
}
impl<R> PanelSignatureVerifierV2<R> {
    pub fn new(authority: R) -> Self {
        Self { authority }
    }
    pub fn authority(&self) -> &R {
        &self.authority
    }
}
impl<R: PanelFrameAuthorityV2> PanelSignatureVerifierV2<R> {
    fn resolved_limit(&self, result: &MinerResultCommit, witness: &PanelWitnessCommit) -> Result<u64, PanelCarrierErrorV2<R::Error>> {
        witness.validate_binding(result).map_err(PanelCarrierErrorV2::Contract)?;
        let limit = self.authority.authorize(result, witness).map_err(PanelCarrierErrorV2::Authority)?;
        if limit == u64::MAX || limit < (EVIDENCE_HEADER_BYTES + SIGNATURE_BYTES) as u64 {
            return Err(PanelCarrierErrorV2::Contract(ContractError::Budget));
        }
        Ok(limit)
    }
}
impl<R: PanelFrameAuthorityV2> CarrierVerifierV2 for PanelSignatureVerifierV2<R> {
    type Authorization = PanelPacketSignatureV2;
    type Error = PanelCarrierErrorV2<R::Error>;

    fn authorize_candidate(&self, result: &MinerResultCommit, witness: &PanelWitnessCommit) -> Result<(), Self::Error> {
        self.resolved_limit(result, witness)?;
        Ok(())
    }
    fn max_carrier_overhead(&self) -> u64 {
        SIGNATURE_BYTES as u64
    }
    fn verify(
        &self,
        result: &MinerResultCommit,
        witness: &PanelWitnessCommit,
        frame: &EvidenceFrame<'_>,
        authorization: &Self::Authorization,
    ) -> Result<u64, Self::Error> {
        let max_signed_bytes = self.resolved_limit(result, witness)?;
        let authenticated = frame
            .authenticate(witness, &authorization.key, &authorization.signature, max_signed_bytes)
            .map_err(PanelCarrierErrorV2::Contract)?;
        Ok(authenticated.frame().signed_wire_bytes())
    }
}
