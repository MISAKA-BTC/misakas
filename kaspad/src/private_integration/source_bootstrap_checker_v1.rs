//! Explicit private first-source checker. One immutable work, own drawn slot
//! and fee input are configured per worker. Local replay is only the input to
//! this checker's approval; the chain still requires the selected quorum.
use super::kernel_paid_producer_v4::{hash, owned_dir, publish_borsh, read_bounded, regular, seed};
use crate::palw_native_custody_v3::PalwNativeCustodyContextV3;
use kaspa_consensus_core::{
    Hash64,
    palw_native_resource_v1::{
        NativeDemandLeaseV1, NativeRetainedV1, preflight_native_value_v1, reserve_native_requests_v1, retain_native_value_decode_v1,
    },
    palw_source_bootstrap_v1::{
        PALW_SELECTED_SOURCE_BOOTSTRAP_MLDSA87_CONTEXT_V1, PalwSourceBootstrapActionV1, palw_source_bootstrap_message_v1,
    },
    palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2},
    tx::{Transaction, TransactionOutpoint},
};
use misaka_palw_kernel::{ledger::ClaimBodyV1, public::TensorWireV1};
use std::{
    collections::{BTreeMap, HashSet},
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

type Parameters = BTreeMap<(u16, Option<u16>), TensorWireV1>;

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrivateSourceBootstrapCheckerSpecV1 {
    version: u16,
    work: String,
    slot: u16,
    checker_bond: String,
    checker_key: PathBuf,
    /// Canonical Borsh Parameters. They must open to the own registered class.
    model: PathBuf,
    payer_key: PathBuf,
    funding_outpoint: String,
    maximum_fee_sompi: u64,
    maximum_input_bytes: u64,
    maximum_workspace_bytes: u64,
    #[serde(skip)]
    _owner: Option<NativeDemandLeaseV1>,
}
impl PrivateSourceBootstrapCheckerSpecV1 {
    pub fn read(path: &Path) -> Result<Self, String> {
        let (raw, _owner) = read_bounded(path, 1 << 20)?;
        let metadata = (raw.len() as u64)
            .checked_mul(8)
            .and_then(|n| n.checked_add(std::mem::size_of::<Self>() as u64))
            .ok_or("source checker metadata overflow")?;
        let owner = reserve_native_requests_v1(metadata).map_err(|e| e.to_string())?;
        let mut value: Self = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
        if value.version != 1 || value.maximum_fee_sompi == 0 || value.maximum_input_bytes == 0 || value.maximum_workspace_bytes == 0 {
            return Err("source checker needs explicit finite input/workspace/fee limits".into());
        }
        value.work = hash(&value.work)?.to_string();
        for path in [&value.checker_key, &value.model, &value.payer_key] {
            regular(path)?;
        }
        value._owner = owner;
        Ok(value)
    }

    pub(super) fn prepare(self, ctx: &PalwNativeCustodyContextV3) -> Result<PreparedSourceBootstrapCheckerV1, String> {
        if ctx.config.params.palw_selected_source_bootstrap_v1.is_none() {
            return Err("source checker needs its explicitly configured dormant private profile".into());
        }
        let checker = PalwBondKeyV2(crate::palw_producer::parse_outpoint(&self.checker_bond)?);
        let funding = crate::palw_producer::parse_outpoint(&self.funding_outpoint)?;
        if checker.0 == funding || kaspa_consensus_core::config::drill::palw_t12_public_genesis_txid_v1(&funding.transaction_id) {
            return Err("source checker needs a separate private carrier input".into());
        }
        let paths = (ctx.data_dir.as_os_str().len() as u64)
            .checked_mul(6)
            .and_then(|n| n.checked_add((self.work.len() + 128 + 64) as u64))
            .and_then(|n| n.checked_add(std::mem::size_of::<PreparedSourceBootstrapCheckerV1>() as u64))
            .ok_or("source checker preparation overflow")?;
        let prepare_owner = reserve_native_requests_v1(paths).map_err(|e| e.to_string())?;
        let key = kaspa_pq_validator_core::ValidatorKey::from_seed(seed(&self.checker_key)?);
        let payer = libcrux_ml_dsa::ml_dsa_87::generate_key_pair(seed(&self.payer_key)?);
        owned_dir(&ctx.data_dir)?;
        let parent = ctx.data_dir.join("source-bootstrap-checker-v1");
        owned_dir(&parent)?;
        let outbox = parent.join(format!(
            "{}-{}-{}",
            self.work,
            faster_hex::hex_string(&kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_bond_id_v1(&checker)),
            self.slot
        ));
        owned_dir(&outbox)?;
        let lock = outbox.join("worker.lock");
        if lock.exists() {
            regular(&lock)?;
        }
        let lock = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(lock).map_err(|e| e.to_string())?;
        lock.try_lock().map_err(|e| format!("source checker outbox is already owned: {e}"))?;
        Ok(PreparedSourceBootstrapCheckerV1 {
            spec: self,
            manager: ctx.consensus_manager.clone(),
            flow: ctx.flow_context.clone(),
            tick: ctx.tick_service.clone(),
            config: ctx.config.clone(),
            checker,
            funding,
            key,
            payer,
            outbox,
            _lock: lock,
            _prepare_owner: prepare_owner,
        })
    }
}

pub(super) struct PreparedSourceBootstrapCheckerV1 {
    spec: PrivateSourceBootstrapCheckerSpecV1,
    manager: Arc<kaspa_consensusmanager::ConsensusManager>,
    flow: Arc<kaspa_p2p_flows::flow_context::FlowContext>,
    tick: Arc<kaspa_core::task::tick::TickService>,
    config: Arc<kaspa_consensus_core::config::Config>,
    checker: PalwBondKeyV2,
    funding: TransactionOutpoint,
    key: kaspa_pq_validator_core::ValidatorKey,
    payer: libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair,
    outbox: PathBuf,
    _lock: fs::File,
    _prepare_owner: Option<NativeDemandLeaseV1>,
}

impl PreparedSourceBootstrapCheckerV1 {
    pub(super) fn start(self) -> Result<(), String> {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
        std::thread::Builder::new()
            .name("private-source-checker-v1".into())
            .spawn(move || {
                if let Err(error) = runtime.block_on(self.run()) {
                    kaspa_core::warn!("private source checker stopped: {error}");
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    async fn run(self) -> Result<(), String> {
        use kaspa_consensus_core::palw_producer_v2::PalwObjectRehearsalV1;
        let id = hash(&self.spec.work)?;
        let (raw, _raw) = read_bounded(&self.spec.model, self.spec.maximum_input_bytes)?;
        let shape = preflight_native_value_v1::<Parameters>(raw.as_slice(), raw.len() as u64).map_err(|e| e.to_string())?;
        if shape.logical_requests_v1() > self.spec.maximum_workspace_bytes {
            return Err("source checker model decode exceeds explicit workspace".into());
        }
        let parameters = retain_native_value_decode_v1(&raw, || borsh::from_slice::<Parameters>(&raw)).map_err(|e| e.to_string())?;
        let object_path = self.outbox.join("approval.borsh");
        let tx_path = self.outbox.join("carrier.borsh");
        let script = kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk(
            &kaspa_hashes::blake2b_512_address_payload(self.payer.verification_key.as_ref()).as_bytes(),
        );
        let mut signed: Option<NativeRetainedV1<PalwConsensusObjectV2>> = None;
        let mut transaction: Option<Transaction> = None;
        let mut tx_owner = None;
        loop {
            if matches!(self.tick.tick(Duration::from_secs(2)).await, kaspa_core::task::tick::TickReason::Shutdown) {
                return Ok(());
            }
            let instance = self.manager.consensus();
            let session = instance.unguarded_session();
            let api = instance.unguarded_session_blocking();
            let observation = match api.palw_source_bootstrap_observation_v1(id) {
                Ok(view) => view,
                Err(error) => {
                    kaspa_core::debug!("source checker own view unavailable; no approval: {error}");
                    continue;
                }
            };
            let state = &observation.state;
            let Some(epoch) = state.source_bootstrap_epoch_v1().map_err(|e| e.to_string())? else { continue };
            let Some(work) = state.source_bootstrap_work_v1(id).map_err(|e| e.to_string())? else { continue };
            if work.approvals.iter().any(|r| r.slot == self.spec.slot) || work.settlement.is_some() {
                continue;
            }
            let Some(seat) = epoch.cohort.get(usize::from(self.spec.slot)) else { continue };
            if epoch.draw_refused
                || observation.virtual_daa > work.work.deadline_daa
                || observation.virtual_daa < work.work.accepted_daa
            {
                continue;
            }
            if seat.bond != kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_bond_id_v1(&self.checker) {
                return Err("configured source checker is not the own frozen cohort's drawn signer".into());
            }
            if state.bond(&self.checker).is_none_or(|b| b.pubkey.as_slice() != self.key.public_key()) {
                return Err("source checker key differs from its own registered native bond".into());
            }
            // Reserve the explicit local ceiling before bounds/plans/replay can
            // allocate. The audited bound must fit it; a refusal signs nothing.
            let _replay = reserve_native_requests_v1(self.spec.maximum_workspace_bytes).map_err(|e| e.to_string())?;
            let ledger = state.source_bootstrap_checker_ledger_v1().map_err(|e| e.to_string())?;
            let claim = ledger.claims.get(&work.work.claim.as_bytes()).ok_or("source own claim absent")?;
            if !matches!(&claim.body, ClaimBodyV1::Program { .. })
                || claim.class_binding_id != work.work.class_binding.as_bytes()
                || claim.convicted
            {
                continue;
            }
            let class = ledger.classes.get(&claim.class_binding_id).ok_or("source own class absent")?;
            let job = ledger.jobs.get(&claim.job_id).ok_or("source own job absent")?;
            let bound = misaka_palw_sdk::claim_paid_v2::producer_resources::bound_program_work_v3(class, job, &parameters)
                .map_err(|e| e.to_string())?;
            if bound.workspace_bytes > u128::from(self.spec.maximum_workspace_bytes)
                || bound.execution_work > u128::from(epoch.context.policy.maximum_check_work)
            {
                continue;
            }
            let mut model = misaka_palw_tir::MapParams::default();
            for (key, tensor) in parameters.iter() {
                model.tensors.insert(*key, tensor.decode().map_err(|e| e.to_string())?);
            }
            if misaka_palw_kernel::trace::ParamCommitmentsV1::of(&model) != class.param_commitments {
                continue;
            }
            match misaka_palw_remote::pursuit::kernel_check_program_v1(&ledger, &work.work.claim.as_bytes(), &model, [0; 64]) {
                Ok(None) => {}
                Ok(Some(_)) => {
                    kaspa_core::warn!("source direct replay found a fault; approval withheld");
                    continue;
                }
                Err(error) => {
                    kaspa_core::debug!("source direct replay unjudged; approval withheld: {error}");
                    continue;
                }
            }
            let latest = match api.palw_source_bootstrap_observation_v1(id) {
                Ok(view) => view,
                Err(_) => continue,
            };
            if latest.import_epoch != observation.import_epoch {
                continue;
            }
            let Some(latest_epoch) = latest.state.source_bootstrap_epoch_v1().map_err(|e| e.to_string())? else { continue };
            let Some(latest_work) = latest.state.source_bootstrap_work_v1(id).map_err(|e| e.to_string())? else { continue };
            // A lengthy full replay may span ordinary blocks. Reuse it only
            // for the same immutable work and fixed cohort on the latest OWN
            // branch; never require a frozen sink to make useful progress.
            if latest_epoch.context != epoch.context
                || latest_epoch.draw_refused
                || latest_work.work != work.work
                || latest_work.settlement.is_some()
                || latest_work.approvals.iter().any(|r| r.slot == self.spec.slot)
                || latest.virtual_daa > work.work.deadline_daa
                || latest.virtual_daa < work.work.accepted_daa
                || latest_epoch.cohort.get(usize::from(self.spec.slot)) != Some(seat)
            {
                continue;
            }
            if signed.is_none() {
                if object_path.exists() {
                    let (raw, _raw) = read_bounded(&object_path, self.spec.maximum_input_bytes)?;
                    signed = Some(retain_native_value_decode_v1(&raw, || borsh::from_slice(&raw)).map_err(|e| e.to_string())?);
                } else {
                    let action = PalwSourceBootstrapActionV1::Approve {
                        work: id,
                        slot: self.spec.slot,
                        statement: work.work.id(),
                        signed_daa: latest.virtual_daa,
                    };
                    let message = palw_source_bootstrap_message_v1(epoch.context.policy.network, self.checker, &action);
                    let _signature = reserve_native_requests_v1(
                        std::mem::size_of::<PalwConsensusObjectV2>() as u64
                            + kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN as u64,
                    )
                    .map_err(|e| e.to_string())?;
                    let object = PalwConsensusObjectV2::SourceBootstrapV1 {
                        actor: self.checker,
                        action,
                        signature: self
                            .key
                            .sign_with_context(message.as_byte_slice(), PALW_SELECTED_SOURCE_BOOTSTRAP_MLDSA87_CONTEXT_V1)
                            .to_vec(),
                    };
                    publish_borsh(&object_path, &object, self.spec.maximum_input_bytes)?;
                    signed = Some(NativeRetainedV1::try_from_clone_v1(&object).map_err(|e| e.to_string())?);
                }
            }
            let object = signed.as_ref().ok_or("source approval WAL absent")?;
            let PalwConsensusObjectV2::SourceBootstrapV1 { actor, action, signature } = &**object else {
                return Err("source WAL family differs".into());
            };
            let PalwSourceBootstrapActionV1::Approve { work: saved, slot, statement, signed_daa } = action else {
                return Err("source WAL action differs".into());
            };
            if *actor != self.checker
                || *saved != id
                || *slot != self.spec.slot
                || *statement != work.work.id()
                || *signed_daa < work.work.accepted_daa
                || *signed_daa > latest.virtual_daa
                || *signed_daa > work.work.deadline_daa
                || !kaspa_consensus_core::palw_attempt_material_v3::verify_ready_signature_v3(
                    self.key.public_key(),
                    palw_source_bootstrap_message_v1(epoch.context.policy.network, *actor, action).as_byte_slice(),
                    signature,
                    PALW_SELECTED_SOURCE_BOOTSTRAP_MLDSA87_CONTEXT_V1,
                )
            {
                return Err("source exact approval WAL identity/signature differs".into());
            }
            if !matches!(api.palw_object_rehearsal_v1(object), Some(PalwObjectRehearsalV1::Accepted)) {
                continue;
            }
            if transaction.is_none() {
                if tx_path.exists() {
                    let (raw, _raw) = read_bounded(&tx_path, self.spec.maximum_input_bytes)?;
                    let (tx, owner) = super::recovery_ack_carrier_v4::decode_carrier(&raw, object)?;
                    transaction = Some(tx);
                    tx_owner = owner;
                } else {
                    let Some(entry) = api.get_virtual_utxo_entry(self.funding) else { continue };
                    let daa = latest.virtual_daa;
                    let minimum =
                        crate::palw_panel::palw_carrier_fees::palw_lifecycle_funding_minimum_v2(&self.config.params, object, daa);
                    if entry.script_public_key != script
                        || entry.amount < minimum
                        || api.palw_bond_payout_payload_v2(PalwBondKeyV2(self.funding)).is_some()
                        || self.flow.mining_manager().outpoint_is_spent_in_mempool(&self.funding)
                        || !crate::palw_panel::palw_fee_funding_usable_v1(
                            &self.funding,
                            &entry,
                            daa,
                            self.config.params.coinbase_spend_maturity(),
                            None,
                            &HashSet::new(),
                        )
                    {
                        continue;
                    }
                    let owner = super::recovery_ack_carrier_v4::reserve_carrier_build_v4(
                        object,
                        &entry,
                        self.payer.verification_key.as_ref().len(),
                    )?;
                    let tx = crate::palw_panel::palw_carrier_fees::build_lifecycle_carrier_v2(
                        &self.config,
                        &self.payer,
                        object,
                        self.funding,
                        &entry,
                        &[],
                        None,
                        daa,
                    )?;
                    let fee = tx
                        .outputs
                        .iter()
                        .try_fold(0u64, |n, output| n.checked_add(output.value))
                        .and_then(|n| entry.amount.checked_sub(n));
                    if fee.is_none_or(|fee| fee > self.spec.maximum_fee_sompi) {
                        return Err("source candidate carrier exceeds explicit fee cap".into());
                    }
                    publish_borsh(&tx_path, &tx, self.spec.maximum_input_bytes)?;
                    transaction = Some(tx);
                    tx_owner = owner;
                }
            }
            let tx = transaction.as_ref().ok_or("source immutable carrier absent")?;
            if tx.inputs.len() != 1 || tx.inputs[0].previous_outpoint != self.funding {
                return Err("source carrier funding substituted".into());
            }
            let Some(entry) = api.get_virtual_utxo_entry(self.funding) else { continue };
            let fee = tx.outputs.iter().try_fold(0u64, |n, o| n.checked_add(o.value)).and_then(|n| entry.amount.checked_sub(n));
            if entry.script_public_key != script
                || tx.outputs.len() != 1
                || tx.outputs[0].script_public_key != script
                || fee.is_none_or(|fee| fee > self.spec.maximum_fee_sompi)
            {
                return Err("source carrier exceeds explicit payer/change/fee authority; WAL retained".into());
            }
            if api.palw_bond_payout_payload_v2(PalwBondKeyV2(self.funding)).is_some()
                || !crate::palw_panel::palw_fee_funding_usable_v1(
                    &self.funding,
                    &entry,
                    latest.virtual_daa,
                    self.config.params.coinbase_spend_maturity(),
                    None,
                    &HashSet::new(),
                )
            {
                continue;
            }
            let _hold = &tx_owner;
            if self
                .flow
                .mining_manager()
                .clone()
                .has_transaction(tx.id(), kaspa_mining::model::tx_query::TransactionQuery::TransactionsOnly)
                .await
            {
                continue;
            }
            let _relay =
                super::recovery_ack_carrier_v4::reserve_carrier_build_v4(object, &entry, self.payer.verification_key.as_ref().len())?;
            match tokio::time::timeout(
                Duration::from_secs(10),
                self.flow.submit_rpc_transaction(&session, tx.clone(), kaspa_mining::mempool::tx::Orphan::Forbidden),
            )
            .await
            {
                Ok(Ok(())) => {}
                Ok(Err(error)) => kaspa_core::debug!("source exact carrier awaits native acceptance: {error}"),
                Err(_) => kaspa_core::debug!("source exact carrier submission timed out"),
            }
        }
    }
}
