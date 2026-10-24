//! Opens one actually accepted source claim under its own producer identity.
//! Neither the request file nor an optimistic Final authenticates useful work;
//! the independently selected native approvals are still required afterwards.
use super::kernel_paid_producer_v4::{hash, owned_dir, publish_borsh, read_bounded, regular, seed};
use crate::palw_native_custody_v3::PalwNativeCustodyContextV3;
use kaspa_consensus_core::{
    palw_native_resource_v1::{
        NativeDemandLeaseV1, NativeRetainedV1, preflight_native_value_v1, reserve_native_requests_v1, retain_native_value_decode_v1,
    },
    palw_source_bootstrap_v1::{
        PALW_SELECTED_SOURCE_BOOTSTRAP_MLDSA87_CONTEXT_V1, PalwSourceBootstrapActionV1, palw_source_bootstrap_message_v1,
    },
    palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2},
    tx::{Transaction, TransactionOutpoint},
};
use std::{
    collections::{BTreeMap, HashSet},
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrivateSourceBootstrapProducerSpecV1 {
    version: u16,
    claim: String,
    producer_bond: String,
    producer_key: PathBuf,
    payer_key: PathBuf,
    funding_outpoint: String,
    maximum_fee_sompi: u64,
    maximum_input_bytes: u64,
    /// Optional first plain OPV work; no paid class/job/source dependency.
    production: Option<PlainSourceProgramSpecV1>,
    /// Explicit prepaid verifier terms and a separate fee carrier input.
    verifier_funding: Option<SourceVerifierFundingSpecV2>,
    #[serde(skip)]
    _owner: Option<NativeDemandLeaseV1>,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PlainSourceProgramSpecV1 {
    base_job: String,
    /// Canonical Borsh map matching the actual own registered class parameters.
    model: PathBuf,
    /// Explicit persisted producer entropy; never derived from the public claim.
    salt: String,
    seal_funding_outpoint: String,
    claim_funding_outpoint: String,
    maximum_seal_fee_sompi: u64,
    maximum_claim_fee_sompi: u64,
    maximum_workspace_bytes: u64,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceVerifierFundingSpecV2 {
    fee_per_checker_sompi: u64,
    carrier_funding_outpoint: String,
    maximum_carrier_fee_sompi: u64,
}
type Parameters = BTreeMap<(u16, Option<u16>), misaka_palw_kernel::public::TensorWireV1>;
impl PrivateSourceBootstrapProducerSpecV1 {
    pub fn read(path: &Path) -> Result<Self, String> {
        let (raw, _raw) = read_bounded(path, 1 << 20)?;
        let count = (raw.len() as u64)
            .checked_mul(8)
            .and_then(|n| n.checked_add(std::mem::size_of::<Self>() as u64))
            .ok_or("source producer metadata overflow")?;
        let owner = reserve_native_requests_v1(count).map_err(|e| e.to_string())?;
        let mut value: Self = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
        if value.version != 1 || value.maximum_fee_sompi == 0 || value.maximum_input_bytes == 0 {
            return Err("source producer requires explicit version/input/fee authority".into());
        }
        value.claim = hash(&value.claim)?.to_string();
        if let Some(p) = &mut value.production {
            p.base_job = hash(&p.base_job)?.to_string();
            p.salt = hash(&p.salt)?.to_string();
            if hash(&p.salt)?.as_bytes() == [0; 64]
                || p.maximum_workspace_bytes == 0
                || p.maximum_seal_fee_sompi == 0
                || p.maximum_claim_fee_sompi == 0
            {
                return Err("plain source work requires explicit salt/workspace/separate fee ceilings".into());
            }
            regular(&p.model)?;
            let points = [&value.funding_outpoint, &p.seal_funding_outpoint, &p.claim_funding_outpoint]
                .map(|s| crate::palw_producer::parse_outpoint(s))
                .into_iter()
                .collect::<Result<Vec<_>, _>>()?;
            if points.iter().enumerate().any(|(i, a)| points[..i].contains(a)) {
                return Err("seal/reveal/open require three distinct immutable funding inputs".into());
            }
        }
        regular(&value.producer_key)?;
        regular(&value.payer_key)?;
        value._owner = owner;
        Ok(value)
    }
    pub(super) fn prepare(self, ctx: &PalwNativeCustodyContextV3) -> Result<PreparedSourceBootstrapProducerV1, String> {
        if ctx.config.params.palw_selected_source_bootstrap_v1.is_none() {
            return Err("source producer policy absent".into());
        }
        let actor = PalwBondKeyV2(crate::palw_producer::parse_outpoint(&self.producer_bond)?);
        let funding = crate::palw_producer::parse_outpoint(&self.funding_outpoint)?;
        if funding == actor.0 || kaspa_consensus_core::config::drill::palw_t12_public_genesis_txid_v1(&funding.transaction_id) {
            return Err("source opening requires a separate private carrier input".into());
        }
        if let Some(p) = &self.production {
            for raw in [&p.seal_funding_outpoint, &p.claim_funding_outpoint] {
                let point = crate::palw_producer::parse_outpoint(raw)?;
                if point == actor.0 || kaspa_consensus_core::config::drill::palw_t12_public_genesis_txid_v1(&point.transaction_id) {
                    return Err("plain source work funding must be separate private carrier inputs".into());
                }
            }
        }
        if self.verifier_funding.is_some() != ctx.config.params.palw_source_verifier_economics_v2.is_some() {
            return Err("source producer economics needs explicit fee terms exactly when its private policy is supplied".into());
        }
        if let Some(spec) = &self.verifier_funding {
            let point = crate::palw_producer::parse_outpoint(&spec.carrier_funding_outpoint)?;
            if point == funding
                || point == actor.0
                || kaspa_consensus_core::config::drill::palw_t12_public_genesis_txid_v1(&point.transaction_id)
                || spec.fee_per_checker_sompi == 0
                || spec.maximum_carrier_fee_sompi == 0
                || self.production.as_ref().is_some_and(|p| {
                    [&p.seal_funding_outpoint, &p.claim_funding_outpoint]
                        .into_iter()
                        .any(|raw| crate::palw_producer::parse_outpoint(raw).ok() == Some(point))
                })
            {
                return Err("source verifier funding needs explicit fees and a separate private carrier input".into());
            }
        }
        let amount = (ctx.data_dir.as_os_str().len() as u64)
            .checked_mul(6)
            .and_then(|n| n.checked_add(3 * 128 + std::mem::size_of::<PreparedSourceBootstrapProducerV1>() as u64))
            .ok_or("source producer preparation overflow")?;
        let owner = reserve_native_requests_v1(amount).map_err(|e| e.to_string())?;
        let key = kaspa_pq_validator_core::ValidatorKey::from_seed(seed(&self.producer_key)?);
        let payer = libcrux_ml_dsa::ml_dsa_87::generate_key_pair(seed(&self.payer_key)?);
        owned_dir(&ctx.data_dir)?;
        let parent = ctx.data_dir.join("source-bootstrap-producer-v1");
        owned_dir(&parent)?;
        let outbox = parent.join(format!(
            "{}-{}",
            self.claim,
            faster_hex::hex_string(&kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_bond_id_v1(&actor))
        ));
        owned_dir(&outbox)?;
        let lock = outbox.join("worker.lock");
        if lock.exists() {
            regular(&lock)?;
        }
        let lock = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(lock).map_err(|e| e.to_string())?;
        lock.try_lock().map_err(|e| format!("source producer outbox is already owned: {e}"))?;
        Ok(PreparedSourceBootstrapProducerV1 {
            spec: self,
            actor,
            funding,
            key,
            payer,
            outbox,
            manager: ctx.consensus_manager.clone(),
            flow: ctx.flow_context.clone(),
            tick: ctx.tick_service.clone(),
            config: ctx.config.clone(),
            _owner: owner,
            _lock: lock,
        })
    }
}
pub(super) struct PreparedSourceBootstrapProducerV1 {
    spec: PrivateSourceBootstrapProducerSpecV1,
    actor: PalwBondKeyV2,
    funding: TransactionOutpoint,
    key: kaspa_pq_validator_core::ValidatorKey,
    payer: libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair,
    outbox: PathBuf,
    manager: Arc<kaspa_consensusmanager::ConsensusManager>,
    flow: Arc<kaspa_p2p_flows::flow_context::FlowContext>,
    tick: Arc<kaspa_core::task::tick::TickService>,
    config: Arc<kaspa_consensus_core::config::Config>,
    _owner: Option<NativeDemandLeaseV1>,
    _lock: fs::File,
}
impl PreparedSourceBootstrapProducerV1 {
    pub(super) fn start(self) -> Result<(), String> {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
        std::thread::Builder::new()
            .name("private-source-producer-v1".into())
            .spawn(move || {
                if let Err(error) = runtime.block_on(self.run()) {
                    kaspa_core::warn!("private source producer stopped: {error}");
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    async fn run(self) -> Result<(), String> {
        use kaspa_consensus_core::palw_producer_v2::PalwObjectRehearsalV1;
        let claim = hash(&self.spec.claim)?;
        let script = kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk(
            &kaspa_hashes::blake2b_512_address_payload(self.payer.verification_key.as_ref()).as_bytes(),
        );
        let mut active_action = None;
        let mut object: Option<NativeRetainedV1<PalwConsensusObjectV2>> = None;
        let mut transaction: Option<Transaction> = None;
        let mut tx_owner = None;
        loop {
            if matches!(self.tick.tick(Duration::from_secs(2)).await, kaspa_core::task::tick::TickReason::Shutdown) {
                return Ok(());
            }
            let instance = self.manager.consensus();
            let session = instance.unguarded_session();
            let api = instance.unguarded_session_blocking();
            let observation = match api.palw_source_bootstrap_epoch_observation_v1() {
                Ok(view) => view,
                Err(error) => {
                    kaspa_core::debug!("source opening own epoch unavailable: {error}");
                    continue;
                }
            };
            let state = &observation.state;
            let Some(epoch) = state.source_bootstrap_epoch_v1().map_err(|e| e.to_string())? else { continue };
            let policy = epoch.context.policy;
            if epoch.draw_refused || observation.virtual_daa < policy.starts_at_daa || observation.virtual_daa > policy.ends_at_daa {
                continue;
            }
            if state.bond(&self.actor).is_none_or(|b| b.pubkey.as_slice() != self.key.public_key()) {
                return Err("source producer key differs from own registered bond".into());
            }
            if let Some(production) = &self.spec.production {
                if !self.drive_plain_program(production, &observation).await? {
                    continue;
                }
            }
            let work = match state.source_bootstrap_prepare_work_v1(claim) {
                Ok(work) => work,
                Err(error) => {
                    kaspa_core::debug!("source opening actual native claim not ready: {error}");
                    continue;
                }
            };
            if work.producer.as_bytes() != kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_bond_id_v1(&self.actor)
                || state.bond(&self.actor).is_none_or(|b| b.pubkey.as_slice() != self.key.public_key())
            {
                return Err("source opening does not belong to the own registered producer/key".into());
            }
            let accepted = state.source_bootstrap_work_v1(work.id()).map_err(|e| e.to_string())?;
            let (action, carrier_funding, maximum_fee, object_name, carrier_name) = if let Some(accepted) = accepted {
                if accepted.work != work {
                    return Err("native source opening statement substituted".into());
                }
                let Some(spec) = &self.spec.verifier_funding else { continue };
                let Some(economics) = state.source_verifier_economics_epoch_v2().map_err(|e| e.to_string())? else { continue };
                let terms = kaspa_consensus_core::palw_source_verifier_economics_v2::PalwSourceVerifierFeeTermsV2 {
                    version: 2,
                    policy: economics.policy.id(),
                    work: work.id(),
                    sponsor: self.actor,
                    fee_per_checker_sompi: spec.fee_per_checker_sompi,
                };
                if let Some(funding) = state.source_verifier_funding_v2(work.id()).map_err(|e| e.to_string())? {
                    if funding.terms != terms {
                        // Another valid branch may carry another sponsor or terms.
                        // Retain our immutable funding WAL and wait for its view.
                        continue;
                    }
                    continue;
                }
                (
                    PalwSourceBootstrapActionV1::FundVerifierEconomicsV2 { terms },
                    crate::palw_producer::parse_outpoint(&spec.carrier_funding_outpoint)?,
                    spec.maximum_carrier_fee_sompi,
                    "verifier-fund.borsh",
                    "verifier-fund-carrier.borsh",
                )
            } else {
                (
                    PalwSourceBootstrapActionV1::Open { policy: policy.id(), epoch: policy.epoch, claim },
                    self.funding,
                    self.spec.maximum_fee_sompi,
                    "open.borsh",
                    "carrier.borsh",
                )
            };
            let suffix = object_name.len().max(carrier_name.len()).checked_add(1).ok_or("source phase suffix overflow")?;
            let path_capacity = self.outbox.as_os_str().len().checked_add(suffix).ok_or("source phase path capacity overflow")?;
            let path_bytes = (path_capacity as u64).checked_mul(2).ok_or("source phase paths overflow")?;
            let _paths = reserve_native_requests_v1(path_bytes).map_err(|e| e.to_string())?;
            // Exact reservations avoid join's intermediate clone/growth buffers.
            // Both filenames and their separator fit the actual longest suffix.
            let mut object_path = PathBuf::new();
            object_path.try_reserve_exact(path_capacity).map_err(|e| e.to_string())?;
            object_path.push(&self.outbox);
            object_path.push(object_name);
            let mut tx_path = PathBuf::new();
            tx_path.try_reserve_exact(path_capacity).map_err(|e| e.to_string())?;
            tx_path.push(&self.outbox);
            tx_path.push(carrier_name);
            if active_action != Some(action) {
                object = None;
                transaction = None;
                tx_owner = None;
                active_action = Some(action);
            }
            if object.is_none() {
                if object_path.exists() {
                    let (raw, _raw) = read_bounded(&object_path, self.spec.maximum_input_bytes)?;
                    object = Some(retain_native_value_decode_v1(&raw, || borsh::from_slice(&raw)).map_err(|e| e.to_string())?);
                } else {
                    let _signature = reserve_native_requests_v1(
                        std::mem::size_of::<PalwConsensusObjectV2>() as u64
                            + kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN as u64,
                    )
                    .map_err(|e| e.to_string())?;
                    let message = palw_source_bootstrap_message_v1(policy.network, self.actor, &action);
                    let signed = PalwConsensusObjectV2::SourceBootstrapV1 {
                        actor: self.actor,
                        action,
                        signature: self
                            .key
                            .sign_with_context(message.as_byte_slice(), PALW_SELECTED_SOURCE_BOOTSTRAP_MLDSA87_CONTEXT_V1)
                            .to_vec(),
                    };
                    publish_borsh(&object_path, &signed, self.spec.maximum_input_bytes)?;
                    object = Some(NativeRetainedV1::try_from_clone_v1(&signed).map_err(|e| e.to_string())?);
                }
            }
            let signed = object.as_ref().ok_or("source opening WAL absent")?;
            let PalwConsensusObjectV2::SourceBootstrapV1 { actor, action: saved, signature } = &**signed else {
                return Err("source opening WAL family differs".into());
            };
            if *actor != self.actor
                || *saved != action
                || !kaspa_consensus_core::palw_attempt_material_v3::verify_ready_signature_v3(
                    self.key.public_key(),
                    palw_source_bootstrap_message_v1(policy.network, *actor, saved).as_byte_slice(),
                    signature,
                    PALW_SELECTED_SOURCE_BOOTSTRAP_MLDSA87_CONTEXT_V1,
                )
            {
                return Err("source opening immutable WAL/policy/key differs".into());
            }
            // Any later block/reorg is judged again by the real native fold.
            // An old observation or queued carrier never substitutes for it.
            if !matches!(api.palw_object_rehearsal_v1(signed), Some(PalwObjectRehearsalV1::Accepted)) {
                continue;
            }
            if transaction.is_none() {
                if tx_path.exists() {
                    let (raw, _raw) = read_bounded(&tx_path, self.spec.maximum_input_bytes)?;
                    let (tx, owner) = super::recovery_ack_carrier_v4::decode_carrier(&raw, signed)?;
                    transaction = Some(tx);
                    tx_owner = owner;
                } else {
                    let Some(entry) = api.get_virtual_utxo_entry(carrier_funding) else { continue };
                    let daa = observation.virtual_daa;
                    let minimum =
                        crate::palw_panel::palw_carrier_fees::palw_lifecycle_funding_minimum_v2(&self.config.params, signed, daa);
                    if entry.script_public_key != script
                        || entry.amount < minimum
                        || api.palw_bond_payout_payload_v2(PalwBondKeyV2(carrier_funding)).is_some()
                        || self.flow.mining_manager().outpoint_is_spent_in_mempool(&carrier_funding)
                        || !crate::palw_panel::palw_fee_funding_usable_v1(
                            &carrier_funding,
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
                        signed,
                        &entry,
                        self.payer.verification_key.as_ref().len(),
                    )?;
                    let tx = crate::palw_panel::palw_carrier_fees::build_lifecycle_carrier_v2(
                        &self.config,
                        &self.payer,
                        signed,
                        carrier_funding,
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
                    if fee.is_none_or(|fee| fee > maximum_fee) {
                        return Err("source opening carrier exceeds fee authority".into());
                    }
                    publish_borsh(&tx_path, &tx, self.spec.maximum_input_bytes)?;
                    transaction = Some(tx);
                    tx_owner = owner;
                }
            }
            let tx = transaction.as_ref().ok_or("source opening carrier absent")?;
            if tx.inputs.len() != 1 || tx.inputs[0].previous_outpoint != carrier_funding {
                return Err("source opening input substituted".into());
            }
            let Some(entry) = api.get_virtual_utxo_entry(carrier_funding) else { continue };
            let fee =
                tx.outputs.iter().try_fold(0u64, |n, output| n.checked_add(output.value)).and_then(|n| entry.amount.checked_sub(n));
            if entry.script_public_key != script
                || tx.outputs.len() != 1
                || tx.outputs[0].script_public_key != script
                || fee.is_none_or(|fee| fee > maximum_fee)
            {
                return Err("source opening WAL payer/change/fee differs".into());
            }
            if api.palw_bond_payout_payload_v2(PalwBondKeyV2(carrier_funding)).is_some()
                || !crate::palw_panel::palw_fee_funding_usable_v1(
                    &carrier_funding,
                    &entry,
                    observation.virtual_daa,
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
                super::recovery_ack_carrier_v4::reserve_carrier_build_v4(signed, &entry, self.payer.verification_key.as_ref().len())?;
            match tokio::time::timeout(
                Duration::from_secs(10),
                self.flow.submit_rpc_transaction(&session, tx.clone(), kaspa_mining::mempool::tx::Orphan::Forbidden),
            )
            .await
            {
                Ok(Ok(())) => {}
                Ok(Err(error)) => kaspa_core::debug!("source opening exact carrier awaits acceptance: {error}"),
                Err(_) => kaspa_core::debug!("source opening exact carrier submission timed out"),
            }
        }
    }
}

impl PreparedSourceBootstrapProducerV1 {
    /// Produce the first direct Program through the ordinary OPV seal/reveal
    /// lifecycle. Native receipts, settlement and later source approvals still
    /// come from their own consumers; no local file is an acceptance marker.
    async fn drive_plain_program(
        &self,
        spec: &PlainSourceProgramSpecV1,
        observed: &kaspa_consensus_core::palw_source_bootstrap_v1::PalwSourceBootstrapObservationV1,
    ) -> Result<bool, String> {
        use misaka_palw_kernel::route::{KernelRouteObjectV1 as O, SaltedCommitV1};
        let id = hash(&self.spec.claim)?.as_bytes();
        let job_id = hash(&spec.base_job)?.as_bytes();
        let actor = kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_bond_id_v1(&self.actor);
        let salt = hash(&spec.salt)?.as_bytes();
        let state = &observed.state;
        let epoch = state.source_bootstrap_epoch_v1().map_err(|e| e.to_string())?.ok_or("source own epoch absent")?;
        let policy = epoch.context.policy;
        let ledger = match state.source_bootstrap_checker_ledger_v1() {
            Ok(ledger) => ledger,
            Err(error) => {
                kaspa_core::debug!("source producer own ledger unavailable: {error}");
                return Ok(false);
            }
        };
        let route = state.kernel_route().ok_or("source own route absent")?;
        // An enrolled paid job requires a paid reveal and cannot bootstrap its
        // own source. Keep the existing native guard and choose no replacement.
        if route.paid_claim_job_is_enrolled_v3(&job_id)? {
            return Err("first source needs a plain independently posted job, without paid enrollment".into());
        }
        if let Some(claim) = ledger.claims.get(&id) {
            if claim.producer != actor || claim.job_id != job_id || claim.class_binding_id != policy.source_profile.as_bytes() {
                return Err("accepted source claim substitutes the configured actual work".into());
            }
            return Ok(true);
        }
        let job = match ledger.jobs.get(&job_id) {
            Some(job) => job,
            None => return Ok(false),
        };
        if job.class_binding_id != policy.source_profile.as_bytes()
            || ledger.mode_of_class(&job.class_binding_id)
                != misaka_palw_kernel::mode::VerificationModeV1::OptimisticPublicVerification
        {
            return Err("first source actual native job is outside fixed direct OPV profile".into());
        }
        let class = ledger.classes.get(&job.class_binding_id).ok_or("source actual class absent")?;
        let work_path = self.outbox.join("plain-commit.borsh");
        let commit = if work_path.exists() {
            let (raw, _raw) = read_bounded(&work_path, self.spec.maximum_input_bytes)?;
            retain_native_value_decode_v1(&raw, || borsh::from_slice::<SaltedCommitV1>(&raw)).map_err(|e| e.to_string())?
        } else {
            let (raw, _raw) = read_bounded(&spec.model, self.spec.maximum_input_bytes)?;
            let shape = preflight_native_value_v1::<Parameters>(&raw[..], raw.len() as u64).map_err(|e| e.to_string())?;
            if shape.logical_requests_v1() > spec.maximum_workspace_bytes {
                return Err("source model exceeds workspace authority".into());
            }
            let parameters =
                retain_native_value_decode_v1(&raw, || borsh::from_slice::<Parameters>(&raw)).map_err(|e| e.to_string())?;
            // Reserve before bounds/validation/evaluation can allocate. The SDK
            // quotes the actual finite Program, all trace and parameter inputs.
            let _workspace = reserve_native_requests_v1(spec.maximum_workspace_bytes).map_err(|e| e.to_string())?;
            let bound = misaka_palw_sdk::claim_paid_v2::producer_resources::bound_program_work_v3(class, job, &parameters)
                .map_err(|e| e.to_string())?;
            if bound.workspace_bytes > u128::from(spec.maximum_workspace_bytes)
                || bound.execution_work > u128::from(policy.maximum_check_work)
                || bound.material_bytes > u128::from(self.spec.maximum_input_bytes)
            {
                return Err("source actual Program exceeds explicit work/material/workspace contract".into());
            }
            let copy = NativeRetainedV1::try_from_clone_v1(&*parameters).map_err(|e| e.to_string())?;
            let (parameters, _parameters_owner) = copy.into_parts_v1();
            let work = misaka_palw_sdk::claim_paid_v2::private_bootstrap::produce_program_work_from_ledger_v3(
                &ledger, job_id, actor, parameters,
            )
            .map_err(|e| e.to_string())?;
            let SaltedCommitV1::Claim { claim, .. } = &work.commit else {
                return Err("direct source generator returned another scope".into());
            };
            if claim.id() != id {
                return Err("generated actual source differs from explicitly authorized claim identity".into());
            }
            let instance = self.manager.consensus();
            let api = instance.unguarded_session_blocking();
            let latest = match api.palw_source_bootstrap_epoch_observation_v1() {
                Ok(latest) => latest,
                Err(_) => return Ok(false),
            };
            // Ordinary new blocks may arrive during real execution. Require
            // the same own fixed epoch/import and immutable actual job/class,
            // rather than an impossible frozen sink for the whole computation.
            let latest_epoch =
                latest.state.source_bootstrap_epoch_v1().map_err(|e| e.to_string())?.ok_or("source latest epoch absent")?;
            if latest.import_epoch != observed.import_epoch
                || latest_epoch.context != epoch.context
                || latest_epoch.draw_refused
                || latest.virtual_daa >= policy.ends_at_daa
            {
                return Ok(false);
            }
            let latest_ledger = latest.state.source_bootstrap_checker_ledger_v1().map_err(|e| e.to_string())?;
            if latest_ledger.jobs.get(&job_id) != Some(job)
                || !latest_ledger.classes.contains_key(&job.class_binding_id)
                || latest.state.kernel_route().ok_or("source latest route absent")?.paid_claim_job_is_enrolled_v3(&job_id)?
            {
                return Ok(false);
            }
            publish_borsh(&work_path, &work.commit, self.spec.maximum_input_bytes)?;
            NativeRetainedV1::try_from_clone_v1(&work.commit).map_err(|e| e.to_string())?
        };
        let SaltedCommitV1::Claim { claim, evidence, .. } = &*commit else {
            return Err("plain source WAL changed scope".into());
        };
        if claim.id() != id || claim.job_id != job_id || claim.producer_bond != actor {
            return Err("plain source WAL substitutes actual claim/job/producer".into());
        }
        // Program body and SaltedCommit carry the same fields with a one-byte
        // enum tag; the native source byte limit therefore applies exactly.
        if borsh::object_length(&*commit).map_err(|e| e.to_string())? as u64 > policy.maximum_claim_bytes {
            return Err("plain source body exceeds fixed native policy byte ceiling".into());
        }
        let _header = reserve_native_requests_v1(borsh::object_length(&evidence.header).map_err(|e| e.to_string())? as u64)
            .map_err(|e| e.to_string())?;
        if misaka_palw_kernel::hash::object_id(b"MISAKA/PALW/SOURCE-COMPLETE-CHECKER/V1", &evidence.header)
            != policy.checker_scope.as_bytes()
        {
            return Err("plain source checker header differs from fixed epoch scope".into());
        }
        let seal = misaka_palw_kernel::route::claim_seal_v2(&id, &salt);
        match ledger.seals.get(&(job_id, actor)) {
            None => {
                // A previously accepted then expired/removed seal is never
                // replaced by another transaction or fresh delay window.
                let object = O::SealClaim { producer: actor, job: job_id, seal };
                self.relay_plain_stage(observed, &object, "seal", &spec.seal_funding_outpoint, spec.maximum_seal_fee_sompi).await?;
            }
            Some(row) if row.seal != seal => return Err("own native seal belongs to different immutable work".into()),
            Some(row) => {
                let ready = row.daa.checked_add(ledger.policy.claim_seal_delay_daa);
                let expires = row.daa.checked_add(ledger.policy.seal_ttl_daa);
                if ready.is_none_or(|at| observed.virtual_daa < at) || expires.is_none_or(|at| observed.virtual_daa > at) {
                    return Ok(false);
                }
                let copy = NativeRetainedV1::try_from_clone_v1(&*commit).map_err(|e| e.to_string())?;
                let (commit, _copy_owner) = copy.into_parts_v1();
                let object = O::CommitClaimSalted { salt, commit };
                self.relay_plain_stage(observed, &object, "reveal", &spec.claim_funding_outpoint, spec.maximum_claim_fee_sompi)
                    .await?;
            }
        }
        Ok(false)
    }

    async fn relay_plain_stage(
        &self,
        observed: &kaspa_consensus_core::palw_source_bootstrap_v1::PalwSourceBootstrapObservationV1,
        inner: &misaka_palw_kernel::route::KernelRouteObjectV1,
        stage: &str,
        funding: &str,
        fee_cap: u64,
    ) -> Result<(), String> {
        use kaspa_consensus_core::{palw_native_resource_v1::NativeHeapV1, palw_producer_v2::PalwObjectRehearsalV1};
        let instance = self.manager.consensus();
        let session = instance.unguarded_session();
        let api = instance.unguarded_session_blocking();
        let latest = match api.palw_source_bootstrap_epoch_observation_v1() {
            Ok(latest) => latest,
            Err(_) => return Ok(()),
        };
        if !same_observation(observed, &latest) {
            return Ok(());
        }
        let point = crate::palw_producer::parse_outpoint(funding)?;
        let object_path = self.outbox.join(format!("{stage}.borsh"));
        let tx_path = self.outbox.join(format!("{stage}-carrier.borsh"));
        let count = inner
            .native_heap_v1("source plain signing")
            .bytes()
            .ok_or("source signing heap unavailable")?
            .checked_add(
                (borsh::object_length(inner).map_err(|e| e.to_string())? as u64).checked_mul(4).ok_or("source encoding overflow")?,
            )
            .and_then(|n| n.checked_add(std::mem::size_of::<PalwConsensusObjectV2>() as u64))
            .and_then(|n| n.checked_add(kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN as u64))
            .ok_or("source signing capacity overflow")?;
        let _signing = reserve_native_requests_v1(count).map_err(|e| e.to_string())?;
        let bytes = inner.encode();
        if bytes.len() > inner.max_encoded_bytes() || bytes.len() as u64 > self.spec.maximum_input_bytes {
            return Err("plain source object exceeds explicit/native frame ceiling".into());
        }
        let network = kaspa_consensus_core::Hash64::from_bytes(
            observed.state.kernel_route().ok_or("source own kernel route absent")?.header.policy.network_domain,
        );
        let message = kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_route_message_v1(network, &self.actor, &bytes);
        let signed = if object_path.exists() {
            let (raw, _raw) = read_bounded(&object_path, self.spec.maximum_input_bytes)?;
            retain_native_value_decode_v1(&raw, || borsh::from_slice::<PalwConsensusObjectV2>(&raw)).map_err(|e| e.to_string())?
        } else {
            let object = PalwConsensusObjectV2::KernelRouteV1 {
                bytes: bytes.clone(),
                signer: self.actor,
                signature: self
                    .key
                    .sign_with_context(
                        message.as_byte_slice(),
                        kaspa_consensus_core::palw_kernel_route_v1::PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1,
                    )
                    .to_vec(),
            };
            publish_borsh(&object_path, &object, self.spec.maximum_input_bytes)?;
            NativeRetainedV1::try_from_clone_v1(&object).map_err(|e| e.to_string())?
        };
        let PalwConsensusObjectV2::KernelRouteV1 { bytes: saved, signer, signature } = &*signed else {
            return Err("plain source WAL family differs".into());
        };
        if *signer != self.actor
            || *saved != bytes
            || !kaspa_consensus_core::palw_attempt_material_v3::verify_ready_signature_v3(
                self.key.public_key(),
                message.as_byte_slice(),
                signature,
                kaspa_consensus_core::palw_kernel_route_v1::PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1,
            )
        {
            return Err("plain source immutable WAL differs from own work/producer/network".into());
        }
        if !matches!(api.palw_object_rehearsal_v1(&signed), Some(PalwObjectRehearsalV1::Accepted)) {
            return Ok(());
        }
        let Some(entry) = api.get_virtual_utxo_entry(point) else { return Ok(()) };
        let script = kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk(
            &kaspa_hashes::blake2b_512_address_payload(self.payer.verification_key.as_ref()).as_bytes(),
        );
        if entry.script_public_key != script
            || api.palw_bond_payout_payload_v2(PalwBondKeyV2(point)).is_some()
            || !crate::palw_panel::palw_fee_funding_usable_v1(
                &point,
                &entry,
                observed.virtual_daa,
                self.config.params.coinbase_spend_maturity(),
                None,
                &HashSet::new(),
            )
        {
            return Ok(());
        }
        let (tx, _owner) = if tx_path.exists() {
            let (raw, _raw) = read_bounded(&tx_path, self.spec.maximum_input_bytes)?;
            super::recovery_ack_carrier_v4::decode_carrier(&raw, &signed)?
        } else {
            if self.flow.mining_manager().outpoint_is_spent_in_mempool(&point) {
                return Ok(());
            }
            let minimum = crate::palw_panel::palw_carrier_fees::palw_lifecycle_funding_minimum_v2(
                &self.config.params,
                &signed,
                observed.virtual_daa,
            );
            if entry.amount < minimum {
                return Ok(());
            }
            let owner =
                super::recovery_ack_carrier_v4::reserve_carrier_build_v4(&signed, &entry, self.payer.verification_key.as_ref().len())?;
            let tx = crate::palw_panel::palw_carrier_fees::build_lifecycle_carrier_v2(
                &self.config,
                &self.payer,
                &signed,
                point,
                &entry,
                &[],
                None,
                observed.virtual_daa,
            )?;
            let fee =
                tx.outputs.iter().try_fold(0u64, |n, output| n.checked_add(output.value)).and_then(|n| entry.amount.checked_sub(n));
            if fee.is_none_or(|fee| fee > fee_cap) {
                return Err("plain source carrier exceeds its independent stage fee cap".into());
            }
            publish_borsh(&tx_path, &tx, self.spec.maximum_input_bytes)?;
            (tx, owner)
        };
        let fee = tx.outputs.iter().try_fold(0u64, |n, output| n.checked_add(output.value)).and_then(|n| entry.amount.checked_sub(n));
        if tx.inputs.len() != 1
            || tx.inputs[0].previous_outpoint != point
            || tx.outputs.len() != 1
            || tx.outputs[0].script_public_key != script
            || fee.is_none_or(|fee| fee > fee_cap)
        {
            return Err("plain source exact carrier input/fee authority differs".into());
        }
        // Resubmission always uses the exact old spend, including after reorg.
        // No timer/local marker fabricates seal, reveal or source acceptance.
        if self
            .flow
            .mining_manager()
            .clone()
            .has_transaction(tx.id(), kaspa_mining::model::tx_query::TransactionQuery::TransactionsOnly)
            .await
        {
            return Ok(());
        }
        let _relay =
            super::recovery_ack_carrier_v4::reserve_carrier_build_v4(&signed, &entry, self.payer.verification_key.as_ref().len())?;
        match tokio::time::timeout(
            Duration::from_secs(10),
            self.flow.submit_rpc_transaction(&session, tx, kaspa_mining::mempool::tx::Orphan::Forbidden),
        )
        .await
        {
            Ok(Ok(())) => {}
            Ok(Err(error)) => kaspa_core::debug!("plain source exact carrier awaiting native acceptance: {error}"),
            Err(_) => kaspa_core::debug!("plain source exact carrier submission timed out"),
        }
        Ok(())
    }
}

fn same_observation(
    a: &kaspa_consensus_core::palw_source_bootstrap_v1::PalwSourceBootstrapObservationV1,
    b: &kaspa_consensus_core::palw_source_bootstrap_v1::PalwSourceBootstrapObservationV1,
) -> bool {
    a.checkpoint == b.checkpoint && a.virtual_daa == b.virtual_daa && a.import_epoch == b.import_epoch
}
