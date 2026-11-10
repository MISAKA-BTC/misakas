//! Retained plain-output ruling. Only the native BCT2 verifier may create a
//! journal insertion; deserializing this row does not authenticate a ruling.
use crate::palw_court_shared_material_intake_v1::batch::plain_output::{BatchPlainOutputReceiptV2, SignedBatchPlainOutputV2};
use crate::{Hash64, palw_staged_court_state_v1::PalwStagedCourtKeyV1 as Key, palw_state_v2::PalwChainStateV2};
use borsh::{BorshDeserialize, BorshSerialize};

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct RootedBatchPlainOutputReceiptV2 {
    pub(crate) receipt: crate::palw_court_shared_material_intake_v1::batch::plain_output::BatchPlainOutputReceiptV2,
    pub(crate) material_until_daa: u64,
}

impl RootedBatchPlainOutputReceiptV2 {
    pub(crate) fn claim(&self) -> Hash64 {
        self.receipt.signed.claim
    }

    pub(crate) fn check(&self, key: &Key) -> Result<(), String> {
        let signed = &self.receipt.signed;
        let rules = &self.receipt.control_rules;
        rules.check()?;
        signed.signing_message_v2(rules.network_domain)?;
        if *key != Key::CourtBatchPlainOutputReceiptV2(signed.claim)
            || self.receipt.at != rules.at
            || self.receipt.at.block == Hash64::default()
            || self.material_until_daa <= self.receipt.at.daa_score
            || !rules.tir_active
            || self.receipt.control_cost.work == 0
            || self.receipt.control_cost.controls != 1
            || self.receipt.carrier_bytes
                != SignedBatchPlainOutputV2::BYTES + crate::palw_batch_court_v1::carriage::BATCH_PLAIN_OUTPUT_WIRE_HEADER_BYTES_V2
            || self.receipt.control_cost.bytes < BatchPlainOutputReceiptV2::ROOTED_MAX_BYTES
            || self.receipt.carrier_bytes > self.receipt.control_cost.bytes
            || self.receipt.carrier_bytes > rules.court.max_close_bytes()
            || signed.policy != crate::palw_court_shared_tir_root_v1::shared_tir_root_policy_id_v1(rules)?
        {
            return Err("plain Batch Output receipt changes its key, native rules or retention".into());
        }
        Ok(())
    }

    /// Validate retained ancestry and identity. The native insertion verifies
    /// the actual FullIds verdict; a snapshot row alone cannot supply it.
    pub(crate) fn check_context(&self, state: &PalwChainStateV2) -> Result<(), String> {
        self.check(&Key::CourtBatchPlainOutputReceiptV2(self.claim()))?;
        let point = state.last_point().ok_or("plain Batch Output receipt has no accepted snapshot")?;
        let context = crate::palw_court_shared_tir_state_v1::context(state, self.claim())?;
        let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, self.claim())
            .ok_or("plain Batch Output receipt lost its original")?;
        let signed = &self.receipt.signed;
        if context.version != 3
            || context.scope_id != signed.scope
            || context.source_id != signed.source
            || context.material_until_daa != self.material_until_daa
            || frame.plan_v1().source_id != signed.source
            || frame.original_v1().job.tail.is_some()
            || self.receipt.at.daa_score < context.original_accepted_daa
            || self.receipt.at.daa_score >= context.hard_daa
            || self.receipt.at.daa_score > point.daa_score
            || self.receipt.at.blue_score > point.blue_score
            || (self.receipt.at.blue_score == point.blue_score && self.receipt.at != *point)
        {
            return Err("plain Batch Output receipt differs from its accepted original or point".into());
        }
        Ok(())
    }
}
