//! Objective direct-replay findings ride the existing signed native FileProof
//! path. A passing local replay never authenticates source consensus credit.
use super::*;
use kaspa_consensus_core::{
    palw_kernel_route_v1::{PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1, palw_kernel_bond_id_v1, palw_kernel_route_message_v1},
    palw_native_resource_v1::ReservedKernelLedgerV1,
    palw_producer_v2::PalwObjectRehearsalV1,
    palw_source_bootstrap_v1::PalwSourceBootstrapWorkV1,
};
use misaka_palw_kernel::{
    ledger::{LedgerEventV1, ProsecutionV1},
    route::{AuthV1, KernelRouteObjectV1},
};

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ObjectiveRefutationFeeV1 {
    funding_outpoint: String,
    maximum_fee_sompi: u64,
}
impl ObjectiveRefutationFeeV1 {
    pub(super) fn funding(&self, checker: PalwBondKeyV2, approval: TransactionOutpoint) -> Result<TransactionOutpoint, String> {
        let point = crate::palw_producer::parse_outpoint(&self.funding_outpoint)?;
        if self.maximum_fee_sompi == 0
            || point == approval
            || point == checker.0
            || kaspa_consensus_core::config::drill::palw_t12_public_genesis_txid_v1(&point.transaction_id)
        {
            return Err("objective refutation needs an explicit positive fee cap and separate private input".into());
        }
        Ok(point)
    }
}

/// Only an actual conviction authorizes publication. Accepted-but-dismissed
/// filings, refusals and duplicates never become a new objective finding.
fn convicts(ledger: &ReservedKernelLedgerV1, action: &KernelRouteObjectV1, collateral: u64, now: u64) -> Result<bool, String> {
    let KernelRouteObjectV1::FileProof { accuser, claim, .. } = action else { return Err("objective route is not FileProof".into()) };
    let mut candidate = ledger.try_clone_v1().map_err(|e| e.to_string())?;
    let Some(next) = now.checked_add(1) else { return Ok(false) };
    if candidate.begin_block_v1(next).is_err() {
        return Ok(false);
    }
    candidate.try_sync_bond_v1(*accuser, collateral).map_err(|e| e.to_string())?;
    let events = candidate.apply_object_reserved_v1(action, &AuthV1 { signer_bond: *accuser }).map_err(|e| e.to_string())?;
    Ok(events
        .is_ok_and(|events| events.iter().any(|event| matches!(event, LedgerEventV1::Convicted {claim: found, ..} if found == claim))))
}
fn validate_object(
    object: &PalwConsensusObjectV2,
    bytes: &[u8],
    checker: PalwBondKeyV2,
    key: &[u8],
    domain: Hash64,
) -> Result<(), String> {
    let PalwConsensusObjectV2::KernelRouteV1 { bytes: saved, signer, signature } = object else {
        return Err("objective WAL family differs".into());
    };
    if *signer != checker
        || saved != bytes
        || !kaspa_consensus_core::palw_attempt_material_v3::verify_ready_signature_v3(
            key,
            palw_kernel_route_message_v1(domain, signer, saved).as_byte_slice(),
            signature,
            PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1,
        )
    {
        return Err("objective WAL immutable claim/proof/actor/signature differs".into());
    }
    Ok(())
}

fn current_conviction(
    state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
    params: &kaspa_consensus_core::config::params::Params,
    checker: PalwBondKeyV2,
    ledger: &ReservedKernelLedgerV1,
    action: &KernelRouteObjectV1,
    now: u64,
) -> Result<bool, String> {
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        return Err("objective checker native state policy absent".into());
    };
    // Use the configured depth only under its existing audit fence and
    // derive its escape from this same own state's anchor ring.
    let raw_depth = if params.palw_audit_2026_09_23_active_at(now) { params.palw_settled_anchor_depth } else { None };
    let committed = kaspa_consensus_core::palw_state_v2::palw_bond_committed_raw_v1(state, &bundle.state, &checker, now, raw_depth);
    let record = state.bond(&checker).ok_or("objective checker bond absent")?;
    let kernel = ledger.bonds.get(&palw_kernel_bond_id_v1(&checker)).map_or(0, |bond| u128::from(bond.reserved));
    let collateral =
        u128::from(record.collateral).saturating_sub(committed.saturating_sub(kernel)).max(kernel).min(u128::from(record.collateral))
            as u64;
    convicts(ledger, action, collateral, now)
}

impl PreparedSourceBootstrapCheckerV1 {
    pub(super) async fn drive_objective_refutation(
        &self,
        id: Hash64,
        work: &PalwSourceBootstrapWorkV1,
        proof: ProsecutionV1,
    ) -> Result<(), String> {
        let Some(authority) = &self.spec.objective_refutation else {
            kaspa_core::warn!("source objective fault found; explicit refutation funding absent; approval withheld");
            return Ok(());
        };
        let funding = authority.funding(self.checker, self.funding)?;
        let instance = self.manager.consensus();
        let api = instance.unguarded_session_blocking();
        let observation = api.palw_source_bootstrap_observation_v1(id).map_err(|error| error.to_string())?;
        let state = &observation.state;
        let Some(current) = state.source_bootstrap_work_v1(id).map_err(|e| e.to_string())? else { return Ok(()) };
        if current.work != *work {
            return Ok(());
        }
        if state.bond(&self.checker).is_none_or(|b| b.pubkey.as_slice() != self.key.public_key()) {
            return Err("objective checker key differs from native bond".into());
        }
        let ledger = state.source_bootstrap_checker_ledger_v1().map_err(|e| e.to_string())?;
        let action =
            KernelRouteObjectV1::FileProof { accuser: palw_kernel_bond_id_v1(&self.checker), claim: work.claim.as_bytes(), proof };
        let encoded = action.encoded_len() as u64;
        if encoded > self.spec.maximum_input_bytes {
            return Err("objective filing exceeds explicit input cap; chunk carriage not configured".into());
        }
        if !current_conviction(state, &self.config.params, self.checker, &ledger, &action, observation.virtual_daa)? {
            return Ok(());
        }
        let _encode = reserve_native_requests_v1(encoded).map_err(|e| e.to_string())?;
        let bytes = action.encode();
        let domain = Hash64::from_bytes(ledger.policy.network_domain);
        let path_requests = (self.outbox.as_os_str().len() as u64)
            .checked_add("objective-refutation-carrier.borsh".len() as u64 + 1)
            .and_then(|n| n.checked_mul(2))
            .ok_or("objective refutation WAL path copy overflow")?;
        let _paths = reserve_native_requests_v1(path_requests).map_err(|error| error.to_string())?;
        let object_path = self.outbox.join("objective-refutation.borsh");
        let carrier_path = self.outbox.join("objective-refutation-carrier.borsh");
        let object = if object_path.exists() {
            let (raw, _raw) = read_bounded(&object_path, self.spec.maximum_input_bytes)?;
            retain_native_value_decode_v1(&raw, || borsh::from_slice(&raw)).map_err(|e| e.to_string())?
        } else {
            let _sign = reserve_native_requests_v1(
                encoded
                    + std::mem::size_of::<PalwConsensusObjectV2>() as u64
                    + kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN as u64,
            )
            .map_err(|e| e.to_string())?;
            let signed = PalwConsensusObjectV2::KernelRouteV1 {
                signature: self
                    .key
                    .sign_with_context(
                        palw_kernel_route_message_v1(domain, &self.checker, &bytes).as_byte_slice(),
                        PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1,
                    )
                    .to_vec(),
                signer: self.checker,
                bytes: bytes.clone(),
            };
            let retained = NativeRetainedV1::try_from_clone_v1(&signed).map_err(|e| e.to_string())?;
            publish_borsh(&object_path, &signed, self.spec.maximum_input_bytes)?;
            retained
        };
        validate_object(&object, &bytes, self.checker, self.key.public_key(), domain)?;
        if !matches!(api.palw_object_rehearsal_v1(&object), Some(PalwObjectRehearsalV1::Accepted)) {
            return Ok(());
        }
        let Some(entry) = api.get_virtual_utxo_entry(funding) else { return Ok(()) };
        let script = kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk(
            &kaspa_hashes::blake2b_512_address_payload(self.payer.verification_key.as_ref()).as_bytes(),
        );
        if entry.script_public_key != script {
            return Err("objective funding belongs to another payer".into());
        }
        if api.palw_bond_payout_payload_v2(PalwBondKeyV2(funding)).is_some()
            || !crate::palw_panel::palw_fee_funding_usable_v1(
                &funding,
                &entry,
                observation.virtual_daa,
                self.config.params.coinbase_spend_maturity(),
                None,
                &HashSet::new(),
            )
        {
            return Ok(());
        }
        let _build = super::super::recovery_ack_carrier_v4::reserve_carrier_build_v4(
            &object,
            &entry,
            self.payer.verification_key.as_ref().len(),
        )?;
        let (tx, _owner) = if carrier_path.exists() {
            let (raw, _raw) = read_bounded(&carrier_path, self.spec.maximum_input_bytes)?;
            super::super::recovery_ack_carrier_v4::decode_carrier(&raw, &object)?
        } else {
            if self.flow.mining_manager().outpoint_is_spent_in_mempool(&funding) {
                return Ok(());
            }
            let tx = crate::palw_panel::palw_carrier_fees::build_lifecycle_carrier_v2(
                &self.config,
                &self.payer,
                &object,
                funding,
                &entry,
                &[],
                None,
                observation.virtual_daa,
            )?;
            validate_carrier(&tx, funding, &entry, &script, authority.maximum_fee_sompi)?;
            publish_borsh(&carrier_path, &tx, self.spec.maximum_input_bytes)?;
            (tx, None)
        };
        validate_carrier(&tx, funding, &entry, &script, authority.maximum_fee_sompi)?;
        if self
            .flow
            .mining_manager()
            .clone()
            .has_transaction(tx.id(), kaspa_mining::model::tx_query::TransactionQuery::TransactionsOnly)
            .await
        {
            return Ok(());
        }
        // Recheck after the await against the current own import generation;
        // branch disagreement waits without rewriting the signed WAL or tx.
        let latest_instance = self.manager.consensus();
        let session = latest_instance.unguarded_session();
        let ready = session.with_consensus_api_v1(|api| -> Result<bool, String> {
            let latest = api.palw_source_bootstrap_observation_v1(id).map_err(|error| error.to_string())?;
            if latest.import_epoch != observation.import_epoch {
                return Ok(false);
            }
            let Some(current) = latest.state.source_bootstrap_work_v1(id).map_err(|error| error.to_string())? else {
                return Ok(false);
            };
            if current.work != *work {
                return Ok(false);
            }
            if latest.state.bond(&self.checker).is_none_or(|bond| bond.pubkey.as_slice() != self.key.public_key()) {
                return Err("objective checker key differs from latest native bond".into());
            }
            let ledger = latest.state.source_bootstrap_checker_ledger_v1().map_err(|error| error.to_string())?;
            if !current_conviction(&latest.state, &self.config.params, self.checker, &ledger, &action, latest.virtual_daa)? {
                return Ok(false);
            }
            if !matches!(api.palw_object_rehearsal_v1(&object), Some(PalwObjectRehearsalV1::Accepted)) {
                return Ok(false);
            }
            let Some(entry) = api.get_virtual_utxo_entry(funding) else { return Ok(false) };
            validate_carrier(&tx, funding, &entry, &script, authority.maximum_fee_sompi)?;
            Ok(api.palw_bond_payout_payload_v2(PalwBondKeyV2(funding)).is_none()
                && crate::palw_panel::palw_fee_funding_usable_v1(
                    &funding,
                    &entry,
                    latest.virtual_daa,
                    self.config.params.coinbase_spend_maturity(),
                    None,
                    &HashSet::new(),
                ))
        })?;
        if !ready {
            return Ok(());
        }
        match tokio::time::timeout(
            Duration::from_secs(10),
            self.flow.submit_rpc_transaction(&session, tx, kaspa_mining::mempool::tx::Orphan::Forbidden),
        )
        .await
        {
            Ok(Ok(())) => {}
            Ok(Err(error)) => kaspa_core::debug!("source objective refutation awaits native acceptance: {error}"),
            Err(_) => kaspa_core::debug!("source objective refutation submission timed out"),
        }
        Ok(())
    }
}
fn validate_carrier(
    tx: &Transaction,
    funding: TransactionOutpoint,
    entry: &kaspa_consensus_core::tx::UtxoEntry,
    script: &kaspa_consensus_core::tx::ScriptPublicKey,
    cap: u64,
) -> Result<(), String> {
    let fee = tx.outputs.iter().try_fold(0u64, |n, o| n.checked_add(o.value)).and_then(|n| entry.amount.checked_sub(n));
    if tx.inputs.len() != 1
        || tx.inputs[0].previous_outpoint != funding
        || tx.outputs.len() != 1
        || entry.script_public_key != *script
        || tx.outputs[0].script_public_key != *script
        || cap == 0
        || fee.is_none_or(|n| n > cap)
    {
        return Err("objective carrier input/payer/change/fee authority differs".into());
    }
    Ok(())
}

#[cfg(test)]
#[path = "source_bootstrap_checker_refutation_tests_v1.rs"]
mod tests;
