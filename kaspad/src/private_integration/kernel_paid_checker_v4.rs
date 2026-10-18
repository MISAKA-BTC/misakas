//! Explicit selected-checker adapter for the private paid-v4 profile. Provider
//! signatures authenticate material; only this node's assigned checker key can
//! sign a receipt after its actual direct check. This is not public activation.
use super::kernel_paid_producer_v4::{bounded_funding, hash, owned_dir, publish_borsh, read_bounded, regular, seed};
use super::native_provider::AuthenticatedSocketExchangeV3;
use crate::palw_native_custody_v3::{NativeReadyExchangeV3, PalwNativeCustodyContextV3};
use kaspa_consensus_core::{
    Hash64,
    palw_claim_challenge_v2::{PAID_CLAIM_RECEIPTS_V2, PalwPaidClaimAuthorityV2},
    palw_kernel_material_query_v4::{
        PALW_KERNEL_MATERIAL_QUERY_FRAME_BYTES_V4, PalwKernelMaterialUnitQueryV4, material_provider_generation_from_route_v4,
    },
    palw_kernel_route_v1::{PalwKernelRouteStateV1, palw_kernel_bond_id_v1},
    palw_native_resource_v1::{
        NativeDemandLeaseV1, NativeRetainedV1, preflight_native_value_v1, reserve_native_requests_v1, retain_native_value_decode_v1,
    },
    palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2},
    tx::TransactionOutpoint,
};
use misaka_palw_kernel::{
    claim_compact_v1::native_v4::NativeCompactProofV4,
    claim_paid_v2::{PaidClaimObjectV2, PaidClaimReceiptV2},
    hash::Digest,
    route::KernelRouteObjectV1,
};
use misaka_palw_sdk::claim_paid_v2::{
    VerifiedPaidClaimSourcesV2, paid_claim_round_sources_from_route_v3, paid_claim_sources_from_route_v2,
    private_bootstrap::ProgramPaidBootstrapWorkV3,
    provider_actions_v4::{objective_action_from_provider_check_v4, sign_provider_objective_action_v4},
    provider_reader_v4::ProviderReaderLimitsV4,
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
    compact_record_path: Option<PathBuf>,
    enrollment: String,
    round: u32,
    checker_bond: String,
    checker_key: PathBuf,
    provider_bond: String,
    provider_address: String,
    payer_key: PathBuf,
    #[serde(deserialize_with = "bounded_funding")]
    /// Each distinct input permits at most one immutable action slot. Include
    /// funding for every demand and the eventual receipt; no automatic top-up.
    funding_outpoints: Vec<String>,
    maximum_fee_sompi: u64,
    max_input_bytes: u64,
    max_unit_bytes: u64,
    max_total_fetch_bytes: u64,
    max_workspace_bytes: u64,
    max_requests: u32,
    /// Explicit admission upper bound for funded availability obligations.
    /// Each signature uses min(this bound, own DAA + native response window).
    /// No authorization is inferred from a local timeout.
    challenge_valid_until_daa: Option<u64>,
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
        if let Some(path) = &spec.compact_record_path {
            regular(path)?;
        }
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
fn saved_kernel_object(object: &PalwConsensusObjectV2, actor: PalwBondKeyV2) -> Result<NativeRetainedV1<KernelRouteObjectV1>, String> {
    let PalwConsensusObjectV2::KernelRouteV1 { bytes, signer, .. } = object else {
        return Err("checker saved action is not a kernel carrier".into());
    };
    if *signer != actor {
        return Err("checker saved action signer differs".into());
    }
    let raw = bytes.get(1..).ok_or("checker saved action route version absent")?;
    preflight_native_value_v1::<KernelRouteObjectV1>(raw, raw.len() as u64).map_err(|e| e.to_string())?;
    retain_native_value_decode_v1(raw, || KernelRouteObjectV1::decode(bytes)).map_err(|e| e.to_string())
}
fn validate_saved_object(
    object: &PalwConsensusObjectV2,
    actor: PalwBondKeyV2,
    base: Digest,
    enrollment: Digest,
    round: u32,
    source: &VerifiedPaidClaimSourcesV2,
    provider: PalwBondKeyV2,
    material_consent: Option<Hash64>,
    valid_until: Option<u64>,
) -> Result<(), String> {
    if let PalwConsensusObjectV2::MonetaryMaterialActionV1 { action, signer, .. } = object {
        let kaspa_consensus_core::palw_monetary_material_v1::PalwMonetaryMaterialActionV1::ChallengeCurrentV4 {
            root,
            consent,
            provider: p,
            dependency,
            valid_until_daa,
            ..
        } = &**action
        else {
            return Err("checker WAL is not its explicitly funded current-generation challenge".into());
        };
        if *signer != actor
            || *root != Hash64::from_bytes(base)
            || *dependency != Hash64::from_bytes(base)
            || Some(*consent) != material_consent
            || *p != provider
            || valid_until.is_none_or(|bound| *valid_until_daa > bound)
        {
            return Err(
                "checker material challenge WAL differs from current native generation/actor/deadline; retained without rebinding"
                    .into(),
            );
        }
        return Ok(());
    }
    let decoded = saved_kernel_object(object, actor)?;
    let bond = palw_kernel_bond_id_v1(&actor);
    match &*decoded {
        KernelRouteObjectV1::PaidClaimV2 { object: PaidClaimObjectV2::Receipt { .. } | PaidClaimObjectV2::RoundReceipt { .. } } => {
            let receipt = receipt_from_saved(object, actor)?;
            if receipt.enrollment != enrollment
                || receipt.round != round
                || receipt.assignment != source.lock().assignment
                || receipt.source_context != source.lock().context
                || receipt.sample_seed != source.lock().seed
                || receipt.signer != bond
            {
                return Err("checker receipt WAL belongs to another source/assignment; retained without rebinding".into());
            }
        }
        KernelRouteObjectV1::FileProof { accuser, claim, .. } if *accuser == bond && *claim == base => {}
        KernelRouteObjectV1::FileDemand { demander, claim, stage, .. } if *demander == bond && *claim == base && *stage < 254 => {}
        _ => return Err("checker WAL is not this actor's exact claim receipt/objective filing".into()),
    }
    Ok(())
}
fn court_action_accepted(
    route: &PalwKernelRouteStateV1,
    object: &PalwConsensusObjectV2,
    actor: PalwBondKeyV2,
    base: Digest,
) -> Result<bool, String> {
    if let PalwConsensusObjectV2::MonetaryMaterialActionV1 { action, signer, .. } = object {
        let kaspa_consensus_core::palw_monetary_material_v1::PalwMonetaryMaterialActionV1::ChallengeCurrentV4 {
            root, dependency, ..
        } = &**action
        else {
            return Ok(false);
        };
        if *signer != actor || *root != Hash64::from_bytes(base) || *dependency != Hash64::from_bytes(base) {
            return Err("checker saved material challenge identity differs".into());
        }
        // Only the native applied-statement checkpoint authenticates this
        // exact actor/consent/unit/deadline. The old coordinate projection and
        // a spent funding input establish no such acceptance.
        let checkpoint =
            kaspa_consensus_core::palw_material_challenge_checkpoint_v4::material_challenge_checkpoint_from_route_v4(route, actor)
                .map_err(|why| why.to_string())?;
        return checkpoint
            .map(|c| c.matches(Hash64::from_bytes(route.header.policy.network_domain), actor, action))
            .transpose()
            .map(|matched| matched.unwrap_or(false))
            .map_err(str::to_string);
    }
    let decoded = saved_kernel_object(object, actor)?;
    match &*decoded {
        KernelRouteObjectV1::FileProof { claim, accuser, .. } if *claim == base && *accuser == palw_kernel_bond_id_v1(&actor) => {
            let ledger = route.ledger()?;
            Ok(ledger.claims.get(&base).is_some_and(|row| row.convicted))
        }
        KernelRouteObjectV1::FileDemand { claim, demander, stage, position }
            if *claim == base && *demander == palw_kernel_bond_id_v1(&actor) =>
        {
            let ledger = route.ledger()?;
            // A public served answer satisfies the objective even after its
            // demander row is retired. This is not an exact carrier-fee ACK.
            Ok(ledger.served.contains_key(&(base, *stage, *position))
                || ledger.demands.get(&(base, *stage, *position)).is_some_and(|row| row.demanders.iter().any(|(d, _)| d == demander)))
        }
        _ => Ok(false),
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
        let shape = preflight_native_value_v1::<ProgramPaidBootstrapWorkV3>(&raw, raw.len() as u64).map_err(|e| e.to_string())?;
        if shape.logical_requests_v1() > self.spec.max_workspace_bytes {
            return Err("checker canonical opening exceeds explicit workspace".into());
        }
        let work = retain_native_value_decode_v1(&raw, || borsh::from_slice::<ProgramPaidBootstrapWorkV3>(&raw))
            .map_err(|e| e.to_string())?;
        if work.version != 3 {
            return Err("checker refuses unknown canonical work version".into());
        }
        let base = kaspa_consensus_core::palw_prospective_kernel_material_v4::commit_identity_v4(&work.commit).0;
        let audit = if let Some(path) = &self.spec.compact_record_path {
            let (raw, _raw) = read_bounded(path, self.spec.max_input_bytes)?;
            let shape = preflight_native_value_v1::<NativeCompactProofV4>(&raw, raw.len() as u64).map_err(|e| e.to_string())?;
            if shape.logical_requests_v1() > self.spec.max_workspace_bytes {
                return Err("checker direct-check record exceeds workspace".into());
            }
            Some(retain_native_value_decode_v1(&raw, || borsh::from_slice::<NativeCompactProofV4>(&raw)).map_err(|e| e.to_string())?)
        } else {
            None
        };
        let script = kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk(
            &kaspa_hashes::blake2b_512_address_payload(self.payer.verification_key.as_ref()).as_bytes(),
        );
        let mut reader_key = Some(self.reader_key);
        let mut client = None;
        loop {
            if matches!(self.tick.tick(Duration::from_secs(2)).await, kaspa_core::task::tick::TickReason::Shutdown) {
                return Ok(());
            }
            let instance = self.manager.consensus();
            let api = instance.unguarded_session_blocking();
            let Some(route) = api.palw_kernel_route_v1() else {
                kaspa_core::debug!("checker own native route unavailable; exact WAL retained");
                continue;
            };
            let needs_finality =
                match challenge_history::needs_finalized_view(&self.outbox, self.spec.max_input_bytes, self.checker, base) {
                    Ok(needs) => needs,
                    Err(outbox::CheckerOutboxErrorV4::NativeView(error)) => {
                        kaspa_core::debug!("checker challenge history unknown; exact WAL retained: {error}");
                        continue;
                    }
                    Err(outbox::CheckerOutboxErrorV4::Outbox(error)) => return Err(error),
                };
            let finalized_challenges = if needs_finality {
                match api.palw_material_challenge_finalized_v4(self.checker) {
                    Ok(view) => view,
                    Err(error) => {
                        kaspa_core::debug!("checker own finalized challenge state unavailable; no action: {error}");
                        continue;
                    }
                }
            } else {
                None
            };
            let scanned = match outbox::scan_outbox(
                &self.outbox,
                &self.funding,
                self.spec.max_input_bytes,
                &route,
                self.checker,
                base,
                enrollment,
                self.spec.round,
                finalized_challenges.as_ref(),
            ) {
                Ok(scanned) => scanned,
                Err(outbox::CheckerOutboxErrorV4::NativeView(error)) => {
                    kaspa_core::debug!("checker own canonical action observation unknown; exact WAL retained: {error}");
                    continue;
                }
                Err(outbox::CheckerOutboxErrorV4::Outbox(error)) => return Err(error),
            };
            let outbox::CheckerOutboxV4 {
                slot,
                mut object,
                mut transaction,
                mut transaction_lease,
                used_funding,
                accepted_challenges,
                observed_terminal,
                awaiting_finality,
                exhausted,
                _metadata,
            } = scanned;
            // Continue monitoring the same canonical fact. A later rollback
            // reselects its original WAL; a local completed flag cannot hide it.
            if observed_terminal || awaiting_finality {
                continue;
            }
            let (action_directory, object_path, tx_path) = outbox::action_paths(&self.outbox, slot);
            let a = match route.paid_claim_enrollment_v3(&enrollment) {
                Ok(authority) => authority,
                Err(error) => {
                    kaspa_core::debug!("checker own enrollment unavailable; exact WAL retained: {error}");
                    continue;
                }
            };
            if a.enrollment.base_claim != base {
                return Err("checker opening belongs to another enrolled claim".into());
            }
            match receipt_accepted(&route, &a, self.spec.round, palw_kernel_bond_id_v1(&self.checker)) {
                Ok(true) => {
                    // Native seat receipt remains subject to reorg. It is not
                    // aggregate Final or permission to retire the immutable WAL.
                    continue;
                }
                Ok(false) => {}
                Err(error) => {
                    kaspa_core::debug!("checker own round unavailable; exact WAL retained: {error}");
                    continue;
                }
            }
            let domain = Hash64::from_bytes(route.header.policy.network_domain);
            let material_consent = match material_provider_generation_from_route_v4(&route, Hash64::from_bytes(base), self.provider) {
                Ok(consent) => consent,
                Err(error) => {
                    // A local decoding/resource refusal creates no provider
                    // fault and never causes a receipt/challenge signature.
                    kaspa_core::debug!("private checker current provider generation pending: {error}");
                    continue;
                }
            };
            if object.is_none() && material_consent.is_none() {
                continue;
            }
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
                let failure = std::cell::RefCell::new(None);
                let query = |stage, position| {
                    let query = PalwKernelMaterialUnitQueryV4 {
                        version: 4,
                        root: Hash64::from_bytes(base),
                        consent: material_consent.ok_or("checker serving provider has no current active generation")?,
                        dependency: Hash64::from_bytes(base),
                        provider: self.provider,
                        stage,
                        position,
                    };
                    let authority = api.palw_kernel_material_query_authority_v4(&query, self.checker, self.key.public_key())?;
                    client
                        .as_ref()
                        .ok_or("explicit checker query client absent")?
                        .fetch_kernel_material_unit_classified_v4(&query, &authority)
                        .map_err(|error| {
                            let message = error.to_string();
                            *failure.borrow_mut() = Some((query, error));
                            message
                        })
                };
                let limits = ProviderReaderLimitsV4 {
                    max_unit_bytes: self.spec.max_unit_bytes,
                    max_total_fetch_bytes: self.spec.max_total_fetch_bytes,
                    max_workspace_bytes: self.spec.max_workspace_bytes,
                    max_units: self.spec.max_requests,
                };
                let checked_result = if a.class.interactive_plan.as_ref().is_some_and(|p| p.version == 4) {
                    let policy =
                        self.config.params.palw_claim_compact_v4.as_ref().ok_or("checker compact private policy absent")?.policy;
                    source.verify_compact_using_provider_v4(
                        &work.material,
                        audit.as_ref().ok_or("V4 checker requires exact native direct-check record")?,
                        &policy,
                        limits,
                        query,
                    )
                } else {
                    source.verify_using_provider_v4(&work.material, limits, query)
                };
                let signed = match checked_result {
                    Ok(checked) => {
                        let Some(index) =
                            source.next_provider_objective_action_index_v4(&checked, self.checker).map_err(|e| e.to_string())?
                        else {
                            // All actual missing units have a native own demand
                            // or public answer. This does not make the check
                            // Clean; wait for answers and perform the check anew.
                            continue;
                        };
                        let action = objective_action_from_provider_check_v4(
                            &source,
                            &checked,
                            self.checker,
                            api.get_virtual_daa_score(),
                            index,
                        )
                        .map_err(|e| e.to_string())?;
                        sign_provider_objective_action_v4(&self.key, domain, self.checker, &action).map_err(|e| e.to_string())?
                    }
                    Err(error) => {
                        use kaspa_consensus_core::palw_kernel_material_query_v4::{
                            PalwKernelMaterialFetchErrorV4 as E, monetary_material_challenge_object_v4,
                        };
                        let Some((unit, cause)) = failure.into_inner() else {
                            kaspa_core::debug!(
                                "checker fresh verification/authority unknown; no new obligation or conviction: {error}"
                            );
                            continue;
                        };
                        if !matches!(cause, E::Unavailable(_) | E::InvalidMaterial(_)) {
                            kaspa_core::debug!("provider acquisition {cause}; no provider obligation or conviction");
                            continue;
                        }
                        match challenge_history::has_accepted_unit(
                            &self.outbox,
                            self.spec.max_input_bytes,
                            self.checker,
                            base,
                            &accepted_challenges,
                            &unit,
                        ) {
                            Ok(true) => continue,
                            Ok(false) => {}
                            Err(outbox::CheckerOutboxErrorV4::NativeView(error)) => {
                                kaspa_core::debug!("checker existing obligation unknown; exact WAL retained: {error}");
                                continue;
                            }
                            Err(outbox::CheckerOutboxErrorV4::Outbox(error)) => return Err(error),
                        }
                        let allowed_until = self.spec.challenge_valid_until_daa.ok_or_else(|| {
                            format!("provider unit {}:{}: {cause}; no explicit answer-obligation deadline", unit.stage, unit.position)
                        })?;
                        let now = api.get_virtual_daa_score();
                        let response_window = self
                            .config
                            .params
                            .palw_monetary_material_v1
                            .as_ref()
                            .ok_or("checker monetary material policy absent")?
                            .policy
                            .response_window_daa;
                        let until = allowed_until.min(now.checked_add(response_window).ok_or("checker admission deadline overflow")?);
                        if until <= now {
                            return Err("explicit material challenge authorization deadline already expired".into());
                        }
                        // Reacquire exact native generation before signing. A
                        // failed local quota/Unknown never enters this branch.
                        api.palw_kernel_material_query_authority_v4(&unit, self.checker, self.key.public_key())?;
                        monetary_material_challenge_object_v4(domain, &unit, until, self.checker, |message, context| {
                            Some(self.key.sign_with_context(message, context).to_vec())
                        })?
                    }
                };
                if !matches!(api.palw_object_rehearsal_v1(&signed), Some(PalwObjectRehearsalV1::Accepted)) {
                    continue;
                }
                if exhausted {
                    return Err("checker explicit action/funding budget exhausted; immutable history retained".into());
                }
                if matches!(&*signed, PalwConsensusObjectV2::MonetaryMaterialActionV1 { .. }) {
                    // Baseline survives a crash before signed-object publish;
                    // no transaction can be relayed without both durable files.
                    let Some(own) = api.palw_kernel_route_v1() else {
                        kaspa_core::debug!("checker native route changed before challenge WAL; no publish");
                        continue;
                    };
                    let finalized = match api.palw_material_challenge_finalized_v4(self.checker) {
                        Ok(view) => view,
                        Err(error) => {
                            kaspa_core::debug!("checker finalized baseline unavailable; no publish: {error}");
                            continue;
                        }
                    };
                    match challenge_history::ensure_base(
                        &self.outbox,
                        self.spec.max_input_bytes,
                        &own,
                        self.checker,
                        base,
                        enrollment,
                        self.spec.round,
                        None,
                        finalized.as_ref(),
                    ) {
                        Ok(()) => {}
                        Err(outbox::CheckerOutboxErrorV4::NativeView(error)) => {
                            kaspa_core::debug!("checker challenge baseline unknown; no publish: {error}");
                            continue;
                        }
                        Err(outbox::CheckerOutboxErrorV4::Outbox(error)) => return Err(error),
                    }
                    // Recheck the same bounded immutable history against these
                    // latest own snapshots. A different client using this key
                    // cannot silently insert an unknown predecessor while we
                    // publish a successor from an older scan.
                    match outbox::scan_outbox(
                        &self.outbox,
                        &self.funding,
                        self.spec.max_input_bytes,
                        &own,
                        self.checker,
                        base,
                        enrollment,
                        self.spec.round,
                        finalized.as_ref(),
                    ) {
                        Ok(latest) if latest.slot == slot && latest.object.is_none() && !latest.exhausted => {}
                        Ok(_) => continue,
                        Err(outbox::CheckerOutboxErrorV4::NativeView(error)) => {
                            kaspa_core::debug!("checker challenge history changed before publish: {error}");
                            continue;
                        }
                        Err(outbox::CheckerOutboxErrorV4::Outbox(error)) => return Err(error),
                    }
                }
                owned_dir(&action_directory)?;
                publish_borsh(&object_path, &*signed, self.spec.max_input_bytes)?;
                object = Some(signed);
            }
            let signed = object.as_ref().ok_or("checker receipt WAL absent")?;
            validate_saved_object(
                signed,
                self.checker,
                base,
                enrollment,
                self.spec.round,
                &source,
                self.provider,
                material_consent,
                self.spec.challenge_valid_until_daa,
            )?;
            let signature_ok = match &**signed {
                PalwConsensusObjectV2::KernelRouteV1 { bytes, signature, .. } => {
                    let message =
                        kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_route_message_v1(domain, &self.checker, bytes);
                    kaspa_consensus_core::palw_attempt_material_v3::verify_ready_signature_v3(
                        self.key.public_key(),
                        message.as_byte_slice(),
                        signature,
                        kaspa_consensus_core::palw_kernel_route_v1::PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1,
                    )
                }
                PalwConsensusObjectV2::MonetaryMaterialActionV1 { action, signature, .. } => {
                    let _payload = reserve_native_requests_v1(borsh::object_length(&**action).map_err(|e| e.to_string())? as u64)
                        .map_err(|e| e.to_string())?;
                    let payload = borsh::to_vec(&**action).map_err(|e| e.to_string())?;
                    let message = kaspa_consensus_core::palw_provider_court_v1::palw_provider_court_message_v1(
                        domain,
                        242,
                        &self.checker,
                        &payload,
                    );
                    kaspa_consensus_core::palw_attempt_material_v3::verify_ready_signature_v3(
                        self.key.public_key(),
                        message.as_byte_slice(),
                        signature,
                        kaspa_consensus_core::palw_provider_court_v1::PALW_PROVIDER_COURT_MLDSA87_CONTEXT_V1,
                    )
                }
                _ => false,
            };
            if !signature_ok {
                return Err("checker WAL signature differs from configured native key/purpose".into());
            }
            if !matches!(api.palw_object_rehearsal_v1(signed), Some(PalwObjectRehearsalV1::Accepted)) {
                continue;
            }
            if transaction.is_none() {
                let daa = api.get_virtual_daa_score();
                let locked = std::collections::HashSet::new();
                let minimum =
                    crate::palw_panel::palw_carrier_fees::palw_lifecycle_funding_minimum_v2(&self.config.params, signed, daa);
                let Some((point, entry)) = self.funding.iter().enumerate().find_map(|(i, point)| {
                    if used_funding[i] {
                        return None;
                    }
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
            let Some(entry) = api.get_virtual_utxo_entry(tx.inputs[0].previous_outpoint) else {
                // Reorg/IBD or an unrelated spend does not authorize a new
                // input, signature or action slot. Keep monitoring exact WAL.
                kaspa_core::debug!("checker original UTXO unavailable; exact WAL retained");
                continue;
            };
            let fee = tx
                .outputs
                .iter()
                .try_fold(0u64, |sum, output| sum.checked_add(output.value))
                .and_then(|outputs| entry.amount.checked_sub(outputs));
            if entry.script_public_key != script || fee.is_none_or(|fee| fee > self.spec.maximum_fee_sompi) {
                return Err("checker saved carrier exceeds current payer/fee authorization; exact WAL retained".into());
            }
            let _relay =
                super::recovery_ack_carrier_v4::reserve_carrier_build_v4(signed, &entry, self.payer.verification_key.as_ref().len())?;
            match tokio::time::timeout(
                Duration::from_secs(10),
                self.flow.submit_rpc_transaction(
                    &instance.unguarded_session(),
                    tx.clone(),
                    kaspa_mining::mempool::tx::Orphan::Forbidden,
                ),
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

mod outbox;
mod challenge_history;

#[cfg(test)]
mod tests;
