//! Explicit private all-target paid producer. No shipped Args, implicit payer,
//! publication approval or synthetic source receipt is introduced here.
use crate::palw_native_custody_v3::{PalwNativeCustodyContextV3, PalwNativeExchangeHandleV3};
use kaspa_consensus_core::{
    Hash64,
    palw_native_resource_v1::{
        NativeRetainedV1, preflight_native_value_v1, reserve_native_requests_v1, retain_native_value_decode_v1,
    },
    palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2},
    tx::TransactionOutpoint,
};
use misaka_palw_sdk::claim_paid_v2::{
    private_bootstrap::ProgramPaidBootstrapWorkV3,
    provider_handoff_v4::{ProviderHandoffLimitsV4, build_kernel_provider_handoff_from_candidate_v4},
};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrivateKernelPaidWorkSpecV4 {
    version: u16,
    work_path: PathBuf,
    checkpoint: String,
    paid_class: String,
    paid_job: String,
    salt: String,
    commit_daa: u64,
    expires_daa: u64,
    payer_key: PathBuf,
    #[serde(deserialize_with = "bounded_funding")]
    funding_outpoints: Vec<String>,
    maximum_fee_sompi: u64,
    max_input_bytes: u64,
    max_material_bytes: u64,
    max_workspace_bytes: u64,
    max_units: u32,
    #[serde(skip)]
    _owner: Option<kaspa_consensus_core::palw_native_resource_v1::NativeDemandLeaseV1>,
}
pub(super) fn bounded_funding<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    struct Visitor;
    impl<'de> serde::de::Visitor<'de> for Visitor {
        type Value = Vec<String>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "at most 64 explicit funding outpoints")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let mut out = Vec::with_capacity(64);
            while out.len() < 64 {
                let Some(value) = seq.next_element()? else { return Ok(out) };
                out.push(value);
            }
            if seq.next_element::<serde::de::IgnoredAny>()?.is_some() {
                return Err(serde::de::Error::custom("funding list exceeds 64 before allocation"));
            }
            Ok(out)
        }
    }
    d.deserialize_seq(Visitor)
}
pub(super) fn regular(path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("kernel paid paths must be absolute".into());
    }
    for parent in path.ancestors().skip(1) {
        if fs::symlink_metadata(parent).map_err(|e| e.to_string())?.file_type().is_symlink() {
            return Err("kernel paid path traverses a symlink".into());
        }
    }
    let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err("kernel paid input/key must be a regular non-symlink file".into());
    }
    Ok(())
}
pub(super) fn hash(raw: &str) -> Result<Hash64, String> {
    let mut bytes = [0u8; 64];
    faster_hex::hex_decode(raw.as_bytes(), &mut bytes).map_err(|e| e.to_string())?;
    Ok(Hash64::from_bytes(bytes))
}
pub(super) fn seed(path: &Path) -> Result<[u8; 32], String> {
    regular(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if fs::metadata(path).map_err(|e| e.to_string())?.permissions().mode() & 0o077 != 0 {
            return Err("kernel producer/payer seed must be private (0600)".into());
        }
    }
    let (raw, _raw_lease) = read_bounded(path, 256)?;
    let text = std::str::from_utf8(&raw).map_err(|e| e.to_string())?.trim();
    let mut result = [0u8; 32];
    faster_hex::hex_decode(text.as_bytes(), &mut result).map_err(|e| e.to_string())?;
    Ok(result)
}
pub(super) fn read_bounded(
    path: &Path,
    cap: u64,
) -> Result<(Vec<u8>, Option<kaspa_consensus_core::palw_native_resource_v1::NativeDemandLeaseV1>), String> {
    regular(path)?;
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let n = file.metadata().map_err(|e| e.to_string())?.len();
    if n > cap {
        return Err("kernel paid file exceeds explicit input cap".into());
    }
    let lease = reserve_native_requests_v1(n.checked_add(1).ok_or("kernel paid file size overflow")?).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(usize::try_from(n.checked_add(1).ok_or("input size overflow")?).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    file.take(n + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() as u64 != n {
        return Err("kernel paid input changed while reading".into());
    }
    Ok((bytes, lease))
}
pub(super) fn owned_dir(path: &Path) -> Result<(), String> {
    for parent in path.ancestors().skip(1) {
        if fs::symlink_metadata(parent).map_err(|e| e.to_string())?.file_type().is_symlink() {
            return Err("kernel paid outbox traverses a symlink".into());
        }
    }
    if path.exists() {
        let m = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
        if !m.is_dir() || m.file_type().is_symlink() {
            return Err("kernel paid outbox is not an owned regular directory".into());
        }
    } else {
        fs::create_dir(path).map_err(|e| e.to_string())?;
        fs::File::open(path.parent().ok_or("owned directory parent absent")?).and_then(|f| f.sync_all()).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
fn publish_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if path.exists() {
        return Err("kernel paid outbox already contains this immutable record".into());
    }
    let temporary = path.with_extension("pending");
    if temporary.exists() {
        regular(&temporary)?;
        fs::remove_file(&temporary).map_err(|e| e.to_string())?;
    }
    let mut o = OpenOptions::new();
    o.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    let mut f = o.open(&temporary).map_err(|e| format!("durable kernel outbox {}: {e}", path.display()))?;
    f.write_all(bytes).and_then(|_| f.sync_all()).map_err(|e| e.to_string())?;
    fs::hard_link(&temporary, path).map_err(|e| e.to_string())?;
    fs::File::open(path.parent().ok_or("outbox parent absent")?).and_then(|f| f.sync_all()).map_err(|e| e.to_string())?;
    fs::remove_file(temporary).map_err(|e| e.to_string())?;
    fs::File::open(path.parent().ok_or("outbox parent absent")?).and_then(|f| f.sync_all()).map_err(|e| e.to_string())
}
pub(super) fn publish_borsh<T: borsh::BorshSerialize>(path: &Path, value: &T, cap: u64) -> Result<(), String> {
    let n = borsh::object_length(value).map_err(|e| e.to_string())? as u64;
    if n > cap {
        return Err("kernel paid durable record exceeds explicit input/outbox cap".into());
    }
    let _lease = reserve_native_requests_v1(n).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(usize::try_from(n).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    borsh::to_writer(&mut bytes, value).map_err(|e| e.to_string())?;
    publish_new(path, &bytes)
}
impl PrivateKernelPaidWorkSpecV4 {
    pub fn read(path: &Path) -> Result<Self, String> {
        let (bytes, _raw_lease) = read_bounded(path, 1024 * 1024)?;
        // JSON's unescaped UTF8 payload is no larger than its source. Keep
        // separate two-capacity bounds for the parser's scratch, destination
        // Strings and transient PathBuf conversion, plus fixed bounded slots.
        // The raw file's independent ticket remains held through parsing.
        let strings = (bytes.len() as u64).checked_mul(2).ok_or("spec string scratch overflow")?;
        let parser_and_fields = strings
            .checked_add(strings)
            .and_then(|n| n.checked_add(strings))
            .and_then(|n| n.checked_add(std::mem::size_of::<Self>() as u64))
            .and_then(|n| n.checked_add(64 * (std::mem::size_of::<String>() + std::mem::size_of::<TransactionOutpoint>()) as u64))
            .ok_or("spec metadata allocation overflow")?;
        let owner = reserve_native_requests_v1(parser_and_fields).map_err(|e| e.to_string())?;
        let mut spec: Self = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        spec._owner = owner;
        if spec.version != 4
            || spec.commit_daa > spec.expires_daa
            || spec.maximum_fee_sompi == 0
            || spec.max_input_bytes == 0
            || spec.max_material_bytes == 0
            || spec.max_workspace_bytes == 0
            || spec.max_units == 0
            || spec.funding_outpoints.is_empty()
            || spec.funding_outpoints.len() > 64
        {
            return Err("kernel paid spec has invalid version or explicit bounds".into());
        }
        regular(&spec.work_path)?;
        regular(&spec.payer_key)?;
        for word in [&spec.checkpoint, &spec.paid_class, &spec.paid_job, &spec.salt] {
            hash(word)?;
        }
        Ok(spec)
    }
    pub fn prepare(self, ctx: &PalwNativeCustodyContextV3) -> Result<PreparedKernelPaidWorkV4, String> {
        if ctx.config.params.palw_claim_compact_v4.is_none() || ctx.config.params.palw_material_release_recovery_v2.is_none() {
            return Err("explicit paid-v4/recovery configuration required".into());
        }
        let producer = PalwBondKeyV2(crate::palw_producer::parse_outpoint(
            ctx.producer_bond.as_deref().ok_or("kernel paid producer bond absent")?,
        )?);
        let producer_path = Path::new(ctx.producer_key_path.as_deref().ok_or("kernel paid producer key absent")?);
        regular(producer_path)?;
        let paths = (ctx.data_dir.as_os_str().len() as u64)
            .checked_add(512)
            .and_then(|n| n.checked_mul(8))
            .ok_or("kernel producer path request overflow")?;
        let prepare_bytes = paths
            .checked_add(
                (std::mem::size_of::<kaspa_pq_validator_core::ValidatorKey>()
                    + 2 * std::mem::size_of::<libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair>()
                    + 64 * std::mem::size_of::<TransactionOutpoint>()
                    + 8 * std::mem::size_of::<PalwBondKeyV2>()) as u64,
            )
            .ok_or("kernel producer preparation overflow")?;
        let prepare_lease = reserve_native_requests_v1(prepare_bytes).map_err(|e| e.to_string())?;
        let key = kaspa_pq_validator_core::ValidatorKey::from_seed(seed(producer_path)?);
        let payer = libcrux_ml_dsa::ml_dsa_87::generate_key_pair(seed(&self.payer_key)?);
        let funding = self.funding_outpoints.iter().map(|s| crate::palw_producer::parse_outpoint(s)).collect::<Result<Vec<_>, _>>()?;
        if funding.iter().enumerate().any(|(i, p)| funding[..i].contains(p))
            || funding.iter().any(|o| kaspa_consensus_core::config::drill::palw_t12_public_genesis_txid_v1(&o.transaction_id))
        {
            return Err("kernel paid funding duplicates or names a public genesis UTXO".into());
        }
        if !(2..=8).contains(&ctx.recovery_provider_roster_v4.len())
            || ctx.recovery_provider_roster_v4.windows(2).any(|p| p[0] >= p[1])
        {
            return Err("kernel paid provider roster invalid".into());
        }
        // No caller-chosen outbox path. Each network directory and producer own
        // the directory; another producer cannot overwrite these fixed files.
        owned_dir(&ctx.data_dir)?;
        let outbox = ctx.data_dir.join("kernel-paid-producer-v4");
        owned_dir(&outbox)?;
        let outbox =
            outbox.join(faster_hex::hex_string(&kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_bond_id_v1(&producer)));
        owned_dir(&outbox)?;
        let lock_path = outbox.join("producer.lock");
        if lock_path.exists() {
            regular(&lock_path)?;
        }
        let outbox_lock =
            OpenOptions::new().read(true).write(true).create(true).truncate(false).open(lock_path).map_err(|e| e.to_string())?;
        outbox_lock.try_lock().map_err(|e| format!("kernel paid producer outbox is already owned: {e}"))?;
        Ok(PreparedKernelPaidWorkV4 {
            spec: self,
            manager: ctx.consensus_manager.clone(),
            flow: ctx.flow_context.clone(),
            tick: ctx.tick_service.clone(),
            config: ctx.config.clone(),
            producer,
            key,
            payer,
            funding,
            providers: ctx.recovery_provider_roster_v4.clone(),
            outbox,
            _outbox_lock: outbox_lock,
            _prepare_lease: prepare_lease,
        })
    }
}
pub struct PreparedKernelPaidWorkV4 {
    spec: PrivateKernelPaidWorkSpecV4,
    manager: Arc<kaspa_consensusmanager::ConsensusManager>,
    flow: Arc<kaspa_p2p_flows::flow_context::FlowContext>,
    tick: Arc<kaspa_core::task::tick::TickService>,
    config: Arc<kaspa_consensus_core::config::Config>,
    producer: PalwBondKeyV2,
    key: kaspa_pq_validator_core::ValidatorKey,
    payer: libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair,
    funding: Vec<TransactionOutpoint>,
    providers: Vec<PalwBondKeyV2>,
    outbox: PathBuf,
    _outbox_lock: fs::File,
    _prepare_lease: Option<kaspa_consensus_core::palw_native_resource_v1::NativeDemandLeaseV1>,
}
impl PreparedKernelPaidWorkV4 {
    pub fn start(self, exchange: PalwNativeExchangeHandleV3) -> Result<(), String> {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
        std::thread::Builder::new()
            .name("private-kernel-paid-v4".into())
            .spawn(move || {
                if let Err(e) = runtime.block_on(self.run(exchange)) {
                    kaspa_core::warn!("private kernel paid producer stopped: {e}");
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    async fn run(self, exchange: PalwNativeExchangeHandleV3) -> Result<(), String> {
        use kaspa_consensus_core::{
            palw_material_release_recovery_v2::PalwMaterialRecoveryActionV2, palw_producer_v2::PalwObjectRehearsalV1,
        };
        let (work_bytes, _file_lease) = read_bounded(&self.spec.work_path, self.spec.max_input_bytes)?;
        let shape = preflight_native_value_v1::<ProgramPaidBootstrapWorkV3>(&work_bytes, work_bytes.len() as u64)
            .map_err(|e| e.to_string())?;
        if shape.logical_requests_v1() > self.spec.max_workspace_bytes {
            return Err("kernel paid decoded input exceeds explicit workspace".into());
        }
        let work = retain_native_value_decode_v1(&work_bytes, || borsh::from_slice::<ProgramPaidBootstrapWorkV3>(&work_bytes))
            .map_err(|e| e.to_string())?;
        let (work, _work_lease) = work.into_parts_v1();
        if work.version != 3 {
            return Err("unknown generated native work version".into());
        }
        let root = kaspa_consensus_core::palw_prospective_kernel_material_v4::commit_identity_v4(&work.commit).0;
        let path = self.outbox.join(faster_hex::hex_string(&root));
        owned_dir(&path)?;
        let object_path = path.join("object.borsh");
        let tx_path = path.join("transaction.borsh");
        let class = hash(&self.spec.paid_class)?.as_bytes();
        let job = hash(&self.spec.paid_job)?.as_bytes();
        let salt = hash(&self.spec.salt)?.as_bytes();
        let checkpoint = hash(&self.spec.checkpoint)?;
        let mut object: Option<NativeRetainedV1<PalwConsensusObjectV2>> = if object_path.exists() {
            let (bytes, _raw_lease) = read_bounded(&object_path, self.spec.max_input_bytes)?;
            preflight_native_value_v1::<PalwConsensusObjectV2>(&bytes, bytes.len() as u64).map_err(|e| e.to_string())?;
            Some(retain_native_value_decode_v1(&bytes, || borsh::from_slice(&bytes)).map_err(|e| e.to_string())?)
        } else {
            None
        };
        // Stored object is matched against exact operator work before either
        // funding or retransmission, including paid class/job/salt/producer.
        if let Some(saved) = &object {
            validate_saved(saved, class, job, &root, &salt, self.producer)?;
        }
        let mut handoff = None;
        let mut ingress = None;
        let mut transaction = None;
        let mut transaction_lease = None;
        if tx_path.exists() {
            let saved = object.as_ref().ok_or("durable transaction lacks its signed object")?;
            let (bytes, _raw_lease) = read_bounded(&tx_path, self.spec.max_input_bytes)?;
            let (tx, lease) = super::recovery_ack_carrier_v4::decode_carrier(&bytes, saved)?;
            transaction = Some(tx);
            transaction_lease = lease;
        }
        let script = kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk(
            &kaspa_hashes::blake2b_512_address_payload(self.payer.verification_key.as_ref()).as_bytes(),
        );
        loop {
            if matches!(self.tick.tick(Duration::from_secs(2)).await, kaspa_core::task::tick::TickReason::Shutdown) {
                return Ok(());
            }
            let api = self.manager.consensus().unguarded_session();
            let route = api.palw_kernel_route_v1().ok_or("own native paid route unavailable")?;
            if let Some(authority) = route.paid_claim_for_base_v2(&root)? {
                if authority.enrollment.class != class
                    || authority.enrollment.job != job
                    || authority.enrollment.producer
                        != kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_bond_id_v1(&self.producer)
                {
                    return Err("existing native paid authority differs from explicit work".into());
                }
                kaspa_core::info!("private kernel paid enrollment accepted: {}", faster_hex::hex_string(&root));
                return Ok(());
            }
            if object.is_none() {
                if handoff.is_none() {
                    let candidate = api.palw_kernel_producer_candidate_v4(
                        checkpoint,
                        self.producer,
                        &work.commit,
                        &salt,
                        self.spec.commit_daa,
                        self.spec.expires_daa,
                    )?;
                    let copy = NativeRetainedV1::try_from_clone_v1(&work.commit).map_err(|e| e.to_string())?;
                    let (commit, _copy_lease) = copy.into_parts_v1();
                    handoff = Some(
                        build_kernel_provider_handoff_from_candidate_v4(
                            candidate,
                            self.providers.clone(),
                            commit,
                            salt,
                            &work.material,
                            ProviderHandoffLimitsV4 {
                                max_material_bytes: self.spec.max_material_bytes,
                                max_workspace_bytes: self.spec.max_workspace_bytes,
                                max_units: self.spec.max_units,
                            },
                            &self.key,
                        )
                        .map_err(|e| e.to_string())?,
                    );
                }
                let request = handoff.as_ref().ok_or("kernel handoff absent")?.request();
                if ingress.is_none() {
                    let grant = api.palw_kernel_material_ingress_v4(checkpoint, request)?;
                    exchange.0.ingest_kernel_material_v4(request, &grant)?;
                    ingress = Some(grant);
                }
                let grant = ingress.as_ref().ok_or("kernel ingress absent")?;
                // Own accepted prior funded enrollment. Requiring useful-work
                // finality here would create a first-work bootstrap cycle.
                // Reorgs are rechecked below and journal dependent claims.
                if !api.palw_kernel_material_admission_accepted_v4(request, &grant.enrollment)? {
                    continue;
                }
                let PalwMaterialRecoveryActionV2::FundedEnroll { monetary, .. } = &*grant.enrollment else {
                    return Err("kernel funding draft absent".into());
                };
                // Producer-owned recovery artifacts. Publish the immutable
                // authenticated context and complete actual bundle before the
                // dependent claim is signed; replacement must not rewrite the
                // old intent/roster or trust an arbitrary peer projection.
                publish_borsh(&path.join("material-context-v4.borsh"), &*grant.context, self.spec.max_input_bytes)?;
                publish_borsh(&path.join("material-bundle-v4.borsh"), &request.bundle, self.spec.max_material_bytes)?;
                let copy = NativeRetainedV1::try_from_clone_v1(&request.intent.commit).map_err(|e| e.to_string())?;
                let (commit, _copy_lease) = copy.into_parts_v1();
                let inner = misaka_palw_sdk::claim_paid_v2::paid_claim_commit_object_v2(
                    &route,
                    class,
                    job,
                    commit,
                    salt,
                    grant.verified.authority().commitment_root.as_bytes(),
                    monetary.id().as_bytes(),
                    self.spec.commit_daa,
                )
                .map_err(|e| e.to_string())?;
                use kaspa_consensus_core::palw_native_resource_v1::NativeHeapV1;
                let wire = borsh::object_length(&inner).map_err(|e| e.to_string())? as u64 + 1;
                let copy = inner.native_heap_v1("kernel paid signing decoder").bytes().ok_or("kernel paid signing heap unknown")?;
                let _signed_construction = reserve_native_requests_v1(
                    copy.checked_add(std::mem::size_of::<misaka_palw_kernel::route::KernelRouteObjectV1>() as u64)
                        .and_then(|n| n.checked_add(wire.checked_mul(4)?))
                        .and_then(|n| n.checked_add(kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN as u64))
                        .ok_or("kernel signing workspace overflow")?,
                )
                .map_err(|e| e.to_string())?;
                let signed =
                    misaka_palw_sdk::claim_paid_v2::sign_paid_claim_object_v2(&self.key, grant.network, self.producer, &inner)?;
                let signed = NativeRetainedV1::retain_existing_v1(signed).map_err(|e| e.to_string())?;
                publish_borsh(&object_path, &*signed, self.spec.max_input_bytes)?;
                object = Some(signed);
            }
            let signed = object.as_ref().ok_or("kernel signed object absent")?;
            let consent = validate_saved(signed, class, job, &root, &salt, self.producer)?;
            let PalwConsensusObjectV2::KernelRouteV1 { bytes, signer, signature } = &**signed else {
                return Err("kernel signed carrier kind changed".into());
            };
            let domain = Hash64::from_bytes(route.header.policy.network_domain);
            let message = kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_route_message_v1(domain, signer, bytes);
            if !kaspa_consensus_core::palw_attempt_material_v3::verify_ready_signature_v3(
                self.key.public_key(),
                message.as_byte_slice(),
                signature,
                kaspa_consensus_core::palw_kernel_route_v1::PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1,
            ) {
                return Err("kernel cached carrier signature invalid".into());
            }
            if !api.palw_kernel_material_consent_accepted_v4(Hash64::from_bytes(root), consent)? {
                continue;
            }

            if !matches!(api.palw_object_rehearsal_v1(signed), Some(PalwObjectRehearsalV1::Accepted)) {
                continue;
            }
            if transaction.is_none() {
                let daa = api.get_virtual_daa_score();
                // Explicit private fee funding never spends any registered PALW
                // bond outpoint. Use a point read for the <=64 named coins,
                // rather than cloning the whole validator population.
                let locked = std::collections::HashSet::new();
                let minimum =
                    crate::palw_panel::palw_carrier_fees::palw_lifecycle_funding_minimum_v2(&self.config.params, signed, daa);
                let Some((point, entry)) = self.funding.iter().find_map(|point| {
                    api.get_virtual_utxo_entry(*point)
                        .filter(|e| {
                            e.amount >= minimum
                                && api.palw_bond_payout_payload_v2(PalwBondKeyV2(*point)).is_none()
                                && e.script_public_key == script
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
                let fee = tx.outputs.iter().try_fold(0u64, |n, o| n.checked_add(o.value)).and_then(|v| entry.amount.checked_sub(v));
                if fee.is_none_or(|n| n > self.spec.maximum_fee_sompi) {
                    return Err("kernel claim carrier exceeds explicit payer fee cap".into());
                }
                publish_borsh(&tx_path, &tx, self.spec.max_input_bytes)?;
                transaction = Some(tx);
                transaction_lease = lease;
            }
            let _hold = &transaction_lease;
            let tx = transaction.as_ref().ok_or("kernel durable transaction absent")?;
            if self
                .flow
                .mining_manager()
                .clone()
                .has_transaction(tx.id(), kaspa_mining::model::tx_query::TransactionQuery::TransactionsOnly)
                .await
            {
                continue;
            }
            // Retain the original owner while reserving the independent relay
            // transaction copy. Cold restart always resends this exact spend.
            let _relay = super::recovery_ack_carrier_v4::reserve_carrier_build_v4(
                signed,
                &api.get_virtual_utxo_entry(tx.inputs[0].previous_outpoint)
                    .ok_or("kernel original funding absent; durable transaction retained")?,
                self.payer.verification_key.as_ref().len(),
            )?;
            match tokio::time::timeout(
                Duration::from_secs(10),
                self.flow.submit_rpc_transaction(&api, tx.clone(), kaspa_mining::mempool::tx::Orphan::Forbidden),
            )
            .await
            {
                Ok(Ok(())) => {}
                Ok(Err(e)) => kaspa_core::warn!("private kernel claim durable relay refused: {e}"),
                Err(_) => kaspa_core::warn!("private kernel claim durable relay timed out"),
            }
        }
    }
}
fn validate_saved(
    object: &PalwConsensusObjectV2,
    class: [u8; 64],
    job: [u8; 64],
    claim: &[u8; 64],
    salt: &[u8; 64],
    producer: PalwBondKeyV2,
) -> Result<Hash64, String> {
    let PalwConsensusObjectV2::KernelRouteV1 { bytes, signer, .. } = object else {
        return Err("saved kernel object is not a native carrier".into());
    };
    let raw = bytes.get(1..).ok_or("saved kernel route version absent")?;
    preflight_native_value_v1::<misaka_palw_kernel::route::KernelRouteObjectV1>(raw, raw.len() as u64).map_err(|e| e.to_string())?;
    let inner = retain_native_value_decode_v1(raw, || misaka_palw_kernel::route::KernelRouteObjectV1::decode(bytes))
        .map_err(|e| e.to_string())?;
    let misaka_palw_kernel::route::KernelRouteObjectV1::PaidClaimV2 {
        object: misaka_palw_kernel::claim_paid_v2::PaidClaimObjectV2::Commit { enrollment, commit, salt: saved_salt },
    } = &*inner
    else {
        return Err("saved kernel object is not a paid commit".into());
    };
    if *signer != producer
        || enrollment.class != class
        || enrollment.job != job
        || enrollment.base_claim != *claim
        || saved_salt != salt
        || commit.producer() != kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_bond_id_v1(&producer)
    {
        return Err("saved paid commit substitutes explicit operator work".into());
    }
    Ok(Hash64::from_bytes(enrollment.material_consent))
}
