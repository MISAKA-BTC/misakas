//! Original native credit spent by an ACCEPTED transaction (spend35). Together
//! with audit32 (independent Final) and delivery33 (real coinbase outpoint),
//! this completes redemption evidence from pinned headers only: maturity,
//! prior unspentness and the payee's successful spend are implied by consensus
//! acceptance in the pinned chain block. No producer, Panel or node flag enters.
use crate::native_redemption_v33::AnchoredNativeCreditDeliveryV33;
use kaspa_consensus_core::{
    Hash64,
    config::params::Params,
    header::Header,
    palw_native_spend_v35::{
        CheckedNativeAcceptedSpendV35, NATIVE_SPEND_MAX_ACCEPTED_IDS_V35, NativeAcceptedSpendProofV35,
        verify_native_accepted_spend_v35,
    },
};

/// JSON transport (concrete arrays; the whole-file cap precedes parsing).
pub fn read_accepted_spend_proof_v35(path: &std::path::Path) -> Result<NativeAcceptedSpendProofV35, String> {
    let raw = crate::native_audit_io_v32::read_bounded_file_v32(path, 160 << 20)?;
    let proof: NativeAcceptedSpendProofV35 = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
    if proof.version != 35
        || proof.accepted_ids.is_empty()
        || proof.accepted_ids.len() > NATIVE_SPEND_MAX_ACCEPTED_IDS_V35
        || proof.spend.inputs.is_empty()
        || proof.spend.inputs.len() > 16_384
        || proof.spend.outputs.len() > 16_384
        || proof.spend.payload.len() > 4 << 20
    {
        return Err("native35 raw accepted-spend proof nested bounds differ".into());
    }
    Ok(proof)
}

pub struct AnchoredNativeCreditSpendV35 {
    spend: CheckedNativeAcceptedSpendV35,
    delivery: misaka_palw_kernel::hash::Digest,
}
impl AnchoredNativeCreditSpendV35 {
    pub fn spend(&self) -> &CheckedNativeAcceptedSpendV35 {
        &self.spend
    }
    pub fn root(&self) -> misaka_palw_kernel::hash::Digest {
        misaka_palw_kernel::hash::object_id(
            b"misaka-palw/sdk/pinned-original-credit-accepted-spend/v35",
            &(self.delivery, self.spend.root()),
        )
    }
}

/// `payment_header` is the audited child header that carried delivery33's coinbase.
#[allow(clippy::too_many_arguments)]
pub fn verify_native_credit_spend_v35(
    delivery: &AnchoredNativeCreditDeliveryV33,
    payment_header: &Header,
    proof: &NativeAcceptedSpendProofV35,
    chain: &Header,
    pinned_chain: Hash64,
    selected_parent: &Header,
    pinned_selected_parent: Hash64,
    parameters: &Params,
) -> Result<AnchoredNativeCreditSpendV35, String> {
    let spend = verify_native_accepted_spend_v35(
        delivery.delivery(),
        payment_header,
        proof,
        chain,
        pinned_chain,
        selected_parent,
        pinned_selected_parent,
        parameters,
    )?;
    Ok(AnchoredNativeCreditSpendV35 { spend, delivery: delivery.root() })
}
