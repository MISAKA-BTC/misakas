//! Explicit selected-checker adapter for the private paid-v4 profile. Provider
//! signatures authenticate material; only this node's assigned checker key can
//! sign a receipt after its actual direct check. This is not public activation.
use super::kernel_paid_producer_v4::{bounded_funding, hash, owned_dir, publish_borsh, read_bounded, regular, seed};
use super::native_provider::AuthenticatedSocketExchangeV3;
use crate::palw_native_custody_v3::{NativeReadyExchangeV3, PalwNativeCustodyContextV3};
use kaspa_consensus_core::{
    Hash64,
    palw_claim_challenge_v2::{PAID_CLAIM_RECEIPTS_V2, PalwPaidClaimAuthorityV2},
    palw_kernel_material_query_v4::{PALW_KERNEL_MATERIAL_QUERY_FRAME_BYTES_V4, PalwKernelMaterialUnitQueryV4},
    palw_kernel_route_v1::{PalwKernelRouteStateV1, palw_kernel_bond_id_v1},
    palw_native_resource_v1::{
        NativeDemandLeaseV1, NativeRetainedV1, preflight_native_value_v1, reserve_native_requests_v1, retain_native_value_decode_v1,
    },
    palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2},
    tx::TransactionOutpoint,
};
use misaka_palw_kernel::{
    claim_paid_v2::{PaidClaimObjectV2, PaidClaimReceiptV2},
    hash::Digest,
    route::KernelRouteObjectV1,
};
use misaka_palw_sdk::claim_paid_v2::{
    VerifiedPaidClaimSourcesV2, paid_claim_round_sources_from_route_v3, paid_claim_sources_from_route_v2,
    private_bootstrap::ProgramPaidBootstrapWorkV3, provider_reader_v4::ProviderReaderLimitsV4,
};
use std::{
    fs::{self, OpenOptions},
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrivateKernelPaidCheckSpecV4 {
    version: u16,
    work_path: PathBuf,
    enrollment: String,
    round: u32,
    checker_bond: String,
    checker_key: PathBuf,
    provider_bond: String,
    provider_address: String,
    payer_key: PathBuf,
    #[serde(deserialize_with = "bounded_funding")]
    funding_outpoints: Vec<String>,
    maximum_fee_sompi: u64,
    max_input_bytes: u64,
    max_unit_bytes: u64,
    max_total_fetch_bytes: u64,
    max_workspace_bytes: u64,
    max_requests: u32,
    #[serde(skip)]
    _owner: Option<NativeDemandLeaseV1>,
}
impl PrivateKernelPaidCheckSpecV4 {
    pub fn read(path: &Path) -> Result<Self, String> {
        // Local fixed-schema metadata, capped independently of material. Keep
        // raw/parse owners alive through construction; no whole-model/RSS claim.
        let (raw, _raw) = read_bounded(path, 1 << 20)?;
        let parse_bytes = (raw.len() as u64)
            .checked_mul(8)
            .and_then(|n| n.checked_add(std::mem::size_of::<Self>() as u64))
            .and_then(|n| n.checked_add(64 * std::mem::size_of::<String>() as u64))
            .ok_or("checker JSON metadata capacity overflow")?;
        let owner = reserve_native_requests_v1(parse_bytes).map_err(|e| e.to_string())?;
        let mut spec: Self = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
        if spec.version != 4
            || spec.maximum_fee_sompi == 0
            || spec.funding_outpoints.is_empty()
            || spec.max_input_bytes == 0
            || spec.max_workspace_bytes == 0
            || spec.max_total_fetch_bytes == 0
            || spec.max_requests == 0
            || spec.max_unit_bytes == 0
            || spec.max_unit_bytes > kaspa_consensus_core::palw_kernel_parameter_material_v4::PALW_KERNEL_MATERIAL_UNIT_BYTES_V4
        {
            return Err("checker requires explicit finite version/funding/acquisition bounds".into());
        }
        regular(&spec.work_path)?;
        regular(&spec.checker_key)?;
        regular(&spec.payer_key)?;
        hash(&spec.enrollment)?;
        spec.provider_address.parse::<SocketAddr>().map_err(|e| e.to_string())?;
        spec._owner = owner;
        Ok(spec)
    }
    pub fn prepare(self, ctx: &PalwNativeCustodyContextV3) -> Result<PreparedKernelPaidCheckV4, String> {
        if ctx.config.params.palw_claim_challenge_v2.is_none() || ctx.config.params.palw_material_release_recovery_v2.is_none() {
            return Err("explicit checker needs paid claim and recovery private profiles".into());
        }
        let checker = PalwBondKeyV2(crate::palw_producer::parse_outpoint(&self.checker_bond)?);
        let provider = PalwBondKeyV2(crate::palw_producer::parse_outpoint(&self.provider_bond)?);
        if checker == provider {
            return Err("checker and serving provider must have distinct native identities".into());
        }
        let _prepare = reserve_native_requests_v1(
            (std::mem::size_of::<PreparedKernelPaidCheckV4>()
                + 2 * std::mem::size_of::<kaspa_pq_validator_core::ValidatorKey>()
                + std::mem::size_of::<libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair>()
                + 64 * std::mem::size_of::<TransactionOutpoint>()) as u64,
        )
        .map_err(|e| e.to_string())?;
        let checker_seed = seed(&self.checker_key)?;
        let key = kaspa_pq_validator_core::ValidatorKey::from_seed(checker_seed);
        let reader_key = kaspa_pq_validator_core::ValidatorKey::from_seed(checker_seed);
        let payer = libcrux_ml_dsa::ml_dsa_87::generate_key_pair(seed(&self.payer_key)?);
        let funding = self.funding_outpoints.iter().map(|s| crate::palw_producer::parse_outpoint(s)).collect::<Result<Vec<_>, _>>()?;
        if funding.iter().enumerate().any(|(i, p)| funding[..i].contains(p))
            || funding.iter().any(|p| kaspa_consensus_core::config::drill::palw_t12_public_genesis_txid_v1(&p.transaction_id))
        {
            return Err("checker funding duplicates or names public genesis inputs".into());
        }
        owned_dir(&ctx.data_dir)?;
        let outbox = ctx.data_dir.join("kernel-paid-checker-v4");
        owned_dir(&outbox)?;
        let outbox = outbox.join(faster_hex::hex_string(&palw_kernel_bond_id_v1(&checker)));
        owned_dir(&outbox)?;
        let lock = outbox.join("checker.lock");
        if lock.exists() {
            regular(&lock)?;
        }
        let lock = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(lock).map_err(|e| e.to_string())?;
        lock.try_lock().map_err(|e| format!("checker outbox already owned: {e}"))?;
        let outbox = outbox.join(format!("{}-{}", faster_hex::hex_string(hash(&self.enrollment)?.as_byte_slice()), self.round));
        owned_dir(&outbox)?;
        Ok(PreparedKernelPaidCheckV4 {
            spec: self,
            manager: ctx.consensus_manager.clone(),
            flow: ctx.flow_context.clone(),
            tick: ctx.tick_service.clone(),
            config: ctx.config.clone(),
            checker,
            provider,
            key,
            reader_key,
            payer,
            funding,
            outbox,
            _lock: lock,
            _prepare,
        })
    }
}
pub struct PreparedKernelPaidCheckV4 {
    spec: PrivateKernelPaidCheckSpecV4,
    manager: Arc<kaspa_consensusmanager::ConsensusManager>,
    flow: Arc<kaspa_p2p_flows::flow_context::FlowContext>,
    tick: Arc<kaspa_core::task::tick::TickService>,
    config: Arc<kaspa_consensus_core::config::Config>,
    checker: PalwBondKeyV2,
    provider: PalwBondKeyV2,
    key: kaspa_pq_validator_core::ValidatorKey,
    reader_key: kaspa_pq_validator_core::ValidatorKey,
    payer: libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair,
    funding: Vec<TransactionOutpoint>,
    outbox: PathBuf,
    _lock: fs::File,
    _prepare: Option<NativeDemandLeaseV1>,
}
fn sources(route: PalwKernelRouteStateV1, enrollment: Digest, round: u32) -> Result<VerifiedPaidClaimSourcesV2, String> {
    let authority = route.paid_claim_enrollment_v3(&enrollment)?;
    if authority.class.interactive_plan.is_some() {
        paid_claim_round_sources_from_route_v3(route, enrollment, round).map_err(|e| e.to_string())
    } else if round == 0 {
        let context = authority.context.id();
        paid_claim_sources_from_route_v2(route, &context).map_err(|e| e.to_string())
    } else {
        Err("noninteractive checker has only round zero".into())
    }
}
fn receipt_accepted(route: &PalwKernelRouteStateV1, a: &PalwPaidClaimAuthorityV2, round: u32, signer: Digest) -> Result<bool, String> {
    if a.class.interactive_plan.is_some() {
        Ok(route.paid_claim_round_v3(&a.enrollment.id(), round)?.receipts.iter().any(|r| r.signer == signer))
    } else {
        let id = misaka_palw_kernel::hash::object_id(b"MISAKA/PALW/PAID-CLAIM-RECEIPT/V2", &(a.enrollment.id(), signer));
        Ok(route
            .aux_row::<PaidClaimReceiptV2>(PAID_CLAIM_RECEIPTS_V2, &id)
            .is_some_and(|r| r.signer == signer && r.enrollment == a.enrollment.id()))
    }
}
fn receipt_from_saved(object: &PalwConsensusObjectV2, checker: PalwBondKeyV2) -> Result<PaidClaimReceiptV2, String> {
    let PalwConsensusObjectV2::KernelRouteV1 { bytes, signer, .. } = object else {
        return Err("checker WAL is not a native kernel object".into());
    };
    if *signer != checker {
        return Err("checker WAL signer differs".into());
    }
    let raw = bytes.get(1..).ok_or("checker route version absent")?;
    preflight_native_value_v1::<KernelRouteObjectV1>(raw, raw.len() as u64).map_err(|e| e.to_string())?;
    let decoded = retain_native_value_decode_v1(raw, || KernelRouteObjectV1::decode(bytes)).map_err(|e| e.to_string())?;
    match &*decoded {
        KernelRouteObjectV1::PaidClaimV2 {
            object: PaidClaimObjectV2::Receipt { receipt } | PaidClaimObjectV2::RoundReceipt { receipt },
        } => Ok(receipt.clone()),
        _ => Err("checker WAL is not a paid seat receipt".into()),
    }
}
impl PreparedKernelPaidCheckV4 {
    pub fn start(self) -> Result<(), String> {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
        std::thread::Builder::new()
            .name("private-kernel-paid-checker-v4".into())
            .spawn(move || {
                if let Err(error) = runtime.block_on(self.run()) {
                    kaspa_core::warn!("private selected checker stopped: {error}");
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    async fn run(self) -> Result<(), String> {
        use kaspa_consensus_core::palw_producer_v2::PalwObjectRehearsalV1;
        let enrollment = hash(&self.spec.enrollment)?.as_bytes();
        let (raw, _raw) = read_bounded(&self.spec.work_path, self.spec.max_input_bytes)?;
        let shape = preflight_native_value_v1::<ProgramPaidBootstrapWorkV3>(&raw[..], raw.len() as u64).map_err(|e| e.to_string())?;
        if shape.logical_requests_v1() > self.spec.max_workspace_bytes {
            return Err("checker canonical opening exceeds explicit workspace".into());
        }
        let work = retain_native_value_decode_v1(&raw, || borsh::from_slice::<ProgramPaidBootstrapWorkV3>(&raw))
            .map_err(|e| e.to_string())?;
        if work.version != 3 {
            return Err("checker refuses unknown canonical work version".into());
        }
        let base = kaspa_consensus_core::palw_prospective_kernel_material_v4::commit_identity_v4(&work.commit).0;
        let object_path = self.outbox.join("receipt.borsh");
        let tx_path = self.outbox.join("transaction.borsh");
        let mut object = if object_path.exists() {
            let (raw, _raw) = read_bounded(&object_path, self.spec.max_input_bytes)?;
            preflight_native_value_v1::<PalwConsensusObjectV2>(&raw[..], raw.len() as u64).map_err(|e| e.to_string())?;
            Some(retain_native_value_decode_v1(&raw, || borsh::from_slice::<PalwConsensusObjectV2>(&raw)).map_err(|e| e.to_string())?)
        } else {
            None
        };
        let mut transaction = None;
        let mut transaction_lease = None;
        if tx_path.exists() {
            let signed = object.as_ref().ok_or("checker tx WAL lacks signed receipt")?;
            let (raw, _raw) = read_bounded(&tx_path, self.spec.max_input_bytes)?;
            let (tx, lease) = super::recovery_ack_carrier_v4::decode_carrier(&raw, signed)?;
            transaction = Some(tx);
            transaction_lease = lease;
        }
        let script = kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk(
            &kaspa_hashes::blake2b_512_address_payload(self.payer.verification_key.as_ref()).as_bytes(),
        );
        let mut reader_key = Some(self.reader_key);
        let mut client = None;
        loop {
            if matches!(self.tick.tick(Duration::from_secs(2)).await, kaspa_core::task::tick::TickReason::Shutdown) {
                return Ok(());
            }
            let api = self.manager.consensus().unguarded_session();
            let route = api.palw_kernel_route_v1().ok_or("checker own native route unavailable")?;
            let a = route.paid_claim_enrollment_v3(&enrollment)?;
            if a.enrollment.base_claim != base {
                return Err("checker opening belongs to another enrolled claim".into());
            }
            if receipt_accepted(&route, &a, self.spec.round, palw_kernel_bond_id_v1(&self.checker))? {
                kaspa_core::info!(
                    "private selected checker receipt accepted: {} round {} (not aggregate Final)",
                    self.spec.enrollment,
                    self.spec.round
                );
                return Ok(());
            }
            let domain = Hash64::from_bytes(route.header.policy.network_domain);
            let source = match sources(route, enrollment, self.spec.round) {
                Ok(s) => s,
                Err(error) => {
                    kaspa_core::debug!("private checker future source/assignment pending: {error}");
                    continue;
                }
            };
            if !source.lock().seats.contains(&self.checker) {
                return Err("configured checker is not a randomly assigned live seat".into());
            }
            if client.is_none() {
                client = Some(AuthenticatedSocketExchangeV3::kernel_reader_v4(
                    self.spec.provider_address.parse().map_err(|e: std::net::AddrParseError| e.to_string())?,
                    misaka_palw_remote::provider_transport_v1::ProviderTransportPolicyV1::new(
                        domain,
                        PALW_KERNEL_MATERIAL_QUERY_FRAME_BYTES_V4,
                    ),
                    self.checker,
                    reader_key.take().ok_or("checker reader key absent")?,
                ));
            }
            if object.is_none() {
                let query = |stage, position| {
                    let query = PalwKernelMaterialUnitQueryV4 {
                        version: 4,
                        root: Hash64::from_bytes(base),
                        consent: Hash64::from_bytes(a.enrollment.material_consent),
                        dependency: Hash64::from_bytes(base),
                        provider: self.provider,
                        stage,
                        position,
                    };
                    let authority = api.palw_kernel_material_query_authority_v4(&query, self.checker, self.key.public_key())?;
                    client.as_ref().ok_or("explicit checker query client absent")?.fetch_kernel_material_unit_v4(&query, &authority)
                };
                let limits = ProviderReaderLimitsV4 {
                    max_unit_bytes: self.spec.max_unit_bytes,
                    max_total_fetch_bytes: self.spec.max_total_fetch_bytes,
                    max_workspace_bytes: self.spec.max_workspace_bytes,
                    max_units: self.spec.max_requests,
                };
                // PRINCIPLES §8: a retired compact V4 plan is never checked, receipted or credited.
                if a.class.interactive_plan.as_ref().is_some_and(|p| p.version == 4) {
                    return Err(misaka_palw_kernel::claim_paid_v2::COMPACT_V4_RETIRED_REASON.into());
                }
                let checked = source.verify_using_provider_v4(&work.material, limits, query).map_err(|e| e.to_string())?;
                let now = api.get_virtual_daa_score();
                let inner = checked.receipt(&source, palw_kernel_bond_id_v1(&self.checker), now).map_err(|e| e.to_string())?;
                // The receipt is fixed-width. Reserve inner/outer bytes and
                // MLDSA output before signing; the independently held checker
                // acquisition owner remains alive until this operation ends.
                let n = borsh::object_length(&inner).map_err(|e| e.to_string())? as u64;
                let _sign = reserve_native_requests_v1(
                    n.checked_mul(4)
                        .and_then(|n| n.checked_add(kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN as u64))
                        .ok_or("checker signing size overflow")?,
                )
                .map_err(|e| e.to_string())?;
                let signed = misaka_palw_sdk::claim_paid_v2::sign_paid_claim_object_v2(&self.key, domain, self.checker, &inner)?;
                let signed = NativeRetainedV1::retain_existing_v1(signed).map_err(|e| e.to_string())?;
                if !matches!(api.palw_object_rehearsal_v1(&signed), Some(PalwObjectRehearsalV1::Accepted)) {
                    continue;
                }
                publish_borsh(&object_path, &*signed, self.spec.max_input_bytes)?;
                object = Some(signed);
            }
            let signed = object.as_ref().ok_or("checker receipt WAL absent")?;
            let receipt = receipt_from_saved(signed, self.checker)?;
            if receipt.enrollment != enrollment
                || receipt.round != self.spec.round
                || receipt.assignment != source.lock().assignment
                || receipt.source_context != source.lock().context
                || receipt.sample_seed != source.lock().seed
                || receipt.signer != palw_kernel_bond_id_v1(&self.checker)
            {
                return Err("checker WAL belongs to a different native source/assignment; retained without rebinding".into());
            }
            let PalwConsensusObjectV2::KernelRouteV1 { bytes, signature, .. } = &**signed else { unreachable!() };
            let message = kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_route_message_v1(domain, &self.checker, bytes);
            if !kaspa_consensus_core::palw_attempt_material_v3::verify_ready_signature_v3(
                self.key.public_key(),
                message.as_byte_slice(),
                signature,
                kaspa_consensus_core::palw_kernel_route_v1::PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1,
            ) {
                return Err("checker WAL signature differs from configured native key".into());
            }
            if !matches!(api.palw_object_rehearsal_v1(signed), Some(PalwObjectRehearsalV1::Accepted)) {
                continue;
            }
            if transaction.is_none() {
                let daa = api.get_virtual_daa_score();
                let locked = std::collections::HashSet::new();
                let minimum =
                    crate::palw_panel::palw_carrier_fees::palw_lifecycle_funding_minimum_v2(&self.config.params, signed, daa);
                let Some((point, entry)) = self.funding.iter().find_map(|point| {
                    api.get_virtual_utxo_entry(*point)
                        .filter(|e| {
                            e.amount >= minimum
                                && e.script_public_key == script
                                && api.palw_bond_payout_payload_v2(PalwBondKeyV2(*point)).is_none()
                                && !self.flow.mining_manager().outpoint_is_spent_in_mempool(point)
                                && crate::palw_panel::palw_fee_funding_usable_v1(
                                    point,
                                    e,
                                    daa,
                                    self.config.params.coinbase_spend_maturity(),
                                    None,
                                    &locked,
                                )
                        })
                        .map(|e| (*point, e))
                }) else {
                    continue;
                };
                let lease = super::recovery_ack_carrier_v4::reserve_carrier_build_v4(
                    signed,
                    &entry,
                    self.payer.verification_key.as_ref().len(),
                )?;
                let tx = crate::palw_panel::palw_carrier_fees::build_lifecycle_carrier_v2(
                    &self.config,
                    &self.payer,
                    signed,
                    point,
                    &entry,
                    &[],
                    None,
                    daa,
                )?;
                let fee = tx.outputs.iter().try_fold(0u64, |n, o| n.checked_add(o.value)).and_then(|n| entry.amount.checked_sub(n));
                if fee.is_none_or(|fee| fee > self.spec.maximum_fee_sompi) {
                    return Err("checker carrier exceeds explicit fee cap".into());
                }
                publish_borsh(&tx_path, &tx, self.spec.max_input_bytes)?;
                transaction = Some(tx);
                transaction_lease = lease;
            }
            let _hold = &transaction_lease;
            let tx = transaction.as_ref().ok_or("checker transaction WAL absent")?;
            if self
                .flow
                .mining_manager()
                .clone()
                .has_transaction(tx.id(), kaspa_mining::model::tx_query::TransactionQuery::TransactionsOnly)
                .await
            {
                continue;
            }
            let entry = api
                .get_virtual_utxo_entry(tx.inputs[0].previous_outpoint)
                .ok_or("checker original UTXO unavailable; exact WAL retained")?;
            let _relay =
                super::recovery_ack_carrier_v4::reserve_carrier_build_v4(signed, &entry, self.payer.verification_key.as_ref().len())?;
            match tokio::time::timeout(
                Duration::from_secs(10),
                self.flow.submit_rpc_transaction(&api, tx.clone(), kaspa_mining::mempool::tx::Orphan::Forbidden),
            )
            .await
            {
                Ok(Ok(())) => {}
                Ok(Err(error)) => kaspa_core::warn!("private checker exact carrier refused: {error}"),
                Err(_) => kaspa_core::warn!("private checker exact carrier timed out"),
            }
        }
    }
}
