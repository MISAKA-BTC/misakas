//! Explicit one-slot paid-commit material continuation. Both old WAL files are
//! immutable; a fresh generation has a separately declared input and fee cap.
//! Reorgs select exact old/new transactions, never rebuild or rebind either.
use super::kernel_paid_producer_v4::{bounded_funding, hash, owned_dir, publish_borsh, read_bounded, regular, seed};
use crate::palw_native_custody_v3::PalwNativeCustodyContextV3;
use kaspa_consensus_core::{
    Hash64,
    palw_kernel_route_v1::{
        PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1, PalwKernelRouteStateV1, palw_kernel_bond_id_v1, palw_kernel_route_message_v1,
    },
    palw_native_resource_v1::{
        NativeDecodeErrorV1 as NativeError, NativeDemandLeaseV1, NativeRetainedV1, NativeWireScanV1, preflight_native_value_v1,
        reserve_native_requests_v1, retain_native_value_decode_classified_v1, retain_native_value_decode_v1,
    },
    palw_paid_material_generation_v5::{
        original_paid_bootstrap_commit_message_v7, original_paid_commit_message_v5, paid_material_generation_from_route_v5,
    },
    palw_producer_v2::PalwObjectRehearsalV1,
    palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2 as Object},
    tx::{Transaction, TransactionOutpoint},
};
use misaka_palw_kernel::{
    claim_paid_material_generation_v5::PaidClaimMaterialGenerationV5 as Generation, claim_paid_v2::PaidClaimObjectV2 as Paid,
    route::KernelRouteObjectV1 as Kernel,
};
use std::{
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrivateKernelPaidMaterialGenerationSpecV5 {
    version: u16,
    base_claim: String,
    expected_generation_path: PathBuf,
    original_producer_spec_path: PathBuf,
    payer_key: PathBuf,
    funding_outpoint: String,
    maximum_fee_sompi: u64,
    maximum_total_additional_fee_sompi: u64,
    max_input_bytes: u64,
    max_workspace_bytes: u64,
    #[serde(skip)]
    _owner: Option<NativeDemandLeaseV1>,
}
/// Read only the original producer's declared funding authorization; unknown
/// fields belong to its existing schema and are skipped without Value copies.
#[derive(serde::Deserialize)]
struct OriginalFunding {
    #[serde(deserialize_with = "bounded_funding")]
    funding_outpoints: Vec<String>,
    maximum_fee_sompi: u64,
    payer_key: PathBuf,
    source_lock_funding_outpoint: Option<String>,
    maximum_source_lock_fee_sompi: Option<u64>,
    max_input_bytes: u64,
    max_workspace_bytes: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OriginalFamily {
    LegacyV5,
    ApprovedV7([u8; 64]),
}
fn original_parts(
    inner: &Kernel,
) -> Result<
    (&misaka_palw_kernel::claim_paid_v2::PaidClaimEnrollmentV2, &misaka_palw_kernel::route::SaltedCommitV1, &[u8; 64], OriginalFamily),
    String,
> {
    match inner {
        Kernel::PaidClaimV2 { object: Paid::Commit { enrollment, commit, salt } } => {
            Ok((enrollment, commit, salt, OriginalFamily::LegacyV5))
        }
        Kernel::PaidClaimV2 { object: Paid::CommitWithApprovedBootstrapV6 { enrollment, commit, salt, source_policy } } => {
            Ok((enrollment, commit, salt, OriginalFamily::ApprovedV7(*source_policy)))
        }
        _ => Err("continuation requires an exact original V2/V6 signed commit, never a wrapper or another action".into()),
    }
}
fn decode_original(original: &Object, actor: PalwBondKeyV2) -> Result<NativeRetainedV1<Kernel>, String> {
    let Object::KernelRouteV1 { bytes, signer, .. } = original else {
        return Err("original carrier namespace differs".into());
    };
    if *signer != actor {
        return Err("original carrier actor differs".into());
    }
    let raw = bytes.get(1..).ok_or("original kernel version absent")?;
    preflight_native_value_v1::<Kernel>(raw, raw.len() as u64).map_err(|e| e.to_string())?;
    retain_native_value_decode_v1(raw, || Kernel::decode(bytes)).map_err(|e| e.to_string())
}
fn original_message(
    network: Hash64,
    actor: &PalwBondKeyV2,
    enrollment: &misaka_palw_kernel::claim_paid_v2::PaidClaimEnrollmentV2,
    commit: &misaka_palw_kernel::route::SaltedCommitV1,
    salt: &[u8; 64],
    family: OriginalFamily,
) -> Result<Hash64, String> {
    match family {
        OriginalFamily::LegacyV5 => original_paid_commit_message_v5(network, actor, enrollment, commit, salt),
        OriginalFamily::ApprovedV7(policy) => {
            original_paid_bootstrap_commit_message_v7(network, actor, enrollment, commit, salt, &policy)
        }
    }
}
impl PrivateKernelPaidMaterialGenerationSpecV5 {
    pub fn read(path: &Path) -> Result<Self, String> {
        Self::read_version(path, 5)
    }
    /// Distinct private authorization schema for an original signed V6 commit.
    /// The shared generation metadata remains native version 5 (330 bytes).
    pub fn read_v7(path: &Path) -> Result<Self, String> {
        Self::read_version(path, 7)
    }
    fn read_version(path: &Path, version: u16) -> Result<Self, String> {
        let (bytes, _raw) = read_bounded(path, 1 << 20)?;
        let owner = reserve_native_requests_v1(
            (bytes.len() as u64)
                .checked_mul(8)
                .and_then(|n| n.checked_add(std::mem::size_of::<Self>() as u64))
                .ok_or("V5 config overflow")?,
        )
        .map_err(|e| e.to_string())?;
        let mut spec: Self = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        if spec.version != version
            || !matches!(version, 5 | 7)
            || spec.max_input_bytes == 0
            || spec.max_workspace_bytes == 0
            || spec.max_input_bytes > super::native_provider::PRIVATE_NATIVE_EXCHANGE_MAX_FRAME_V3
            || spec.maximum_fee_sompi == 0
            || spec.maximum_fee_sompi > spec.maximum_total_additional_fee_sompi
        {
            return Err("V5 needs exact version, finite input/workspace and explicit one-slot total fee authorization".into());
        }
        spec.base_claim = hash(&spec.base_claim)?.to_string();
        regular(&spec.expected_generation_path)?;
        regular(&spec.original_producer_spec_path)?;
        regular(&spec.payer_key)?;
        crate::palw_producer::parse_outpoint(&spec.funding_outpoint)?;
        spec._owner = owner;
        Ok(spec)
    }
    fn validate_family(&self, family: OriginalFamily) -> Result<(), String> {
        if !matches!((self.version, family), (5, OriginalFamily::LegacyV5) | (7, OriginalFamily::ApprovedV7(_))) {
            return Err("continuation authorization version differs from exact original signed namespace".into());
        }
        Ok(())
    }
    pub fn prepare(self, ctx: &PalwNativeCustodyContextV3) -> Result<PreparedKernelPaidMaterialGenerationV5, String> {
        if ctx.config.params.palw_claim_challenge_v2.is_none() || ctx.config.params.palw_material_release_recovery_v2.is_none() {
            return Err("V5 private paid/recovery capabilities are absent".into());
        }
        let producer = PalwBondKeyV2(crate::palw_producer::parse_outpoint(
            ctx.producer_bond.as_deref().ok_or("V5 explicit producer bond absent")?,
        )?);
        let prepare = reserve_native_requests_v1(
            (ctx.data_dir.as_os_str().len() as u64 + 2048)
                .checked_mul(8)
                .and_then(|n| {
                    n.checked_add(
                        (std::mem::size_of::<PreparedKernelPaidMaterialGenerationV5>()
                            + 2 * std::mem::size_of::<libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair>()
                            + std::mem::size_of::<kaspa_pq_validator_core::ValidatorKey>()) as u64,
                    )
                })
                .ok_or("V5 preparation overflow")?,
        )
        .map_err(|e| e.to_string())?;
        let key = kaspa_pq_validator_core::ValidatorKey::from_seed(seed(Path::new(
            ctx.producer_key_path.as_deref().ok_or("V5 explicit producer key absent")?,
        ))?);
        let payer = libcrux_ml_dsa::ml_dsa_87::generate_key_pair(seed(&self.payer_key)?);
        let funding = crate::palw_producer::parse_outpoint(&self.funding_outpoint)?;
        if funding == producer.0 || kaspa_consensus_core::config::drill::palw_t12_public_genesis_txid_v1(&funding.transaction_id) {
            return Err("V5 additional input names public genesis funding".into());
        }
        let (raw, _raw) = read_bounded(&self.original_producer_spec_path, 1 << 20)?;
        let original_config_owner = reserve_native_requests_v1(
            (raw.len() as u64)
                .checked_mul(8)
                .and_then(|n| n.checked_add(64 * std::mem::size_of::<String>() as u64))
                .ok_or("V5 old config overflow")?,
        )
        .map_err(|e| e.to_string())?;
        let original_config: OriginalFunding = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
        if original_config.funding_outpoints.is_empty()
            || original_config.maximum_fee_sompi == 0
            || original_config.max_input_bytes == 0
            || original_config.max_workspace_bytes == 0
            || original_config.source_lock_funding_outpoint.is_some() != original_config.maximum_source_lock_fee_sompi.is_some()
            || original_config.maximum_source_lock_fee_sompi == Some(0)
        {
            return Err("V5 original funding declaration is empty".into());
        }
        for point in &original_config.funding_outpoints {
            if crate::palw_producer::parse_outpoint(point)? == funding {
                return Err("V5 additional input reuses original funding authorization".into());
            }
        }
        if original_config
            .source_lock_funding_outpoint
            .as_deref()
            .map(crate::palw_producer::parse_outpoint)
            .transpose()?
            .is_some_and(|point| point == funding)
        {
            return Err("continuation input reuses the original V6 source-lock funding authority".into());
        }
        let old_payer = libcrux_ml_dsa::ml_dsa_87::generate_key_pair(seed(&original_config.payer_key)?);
        owned_dir(&ctx.data_dir)?;
        let parent = ctx.data_dir.join("kernel-paid-producer-v4");
        owned_dir(&parent)?;
        let parent = parent.join(faster_hex::hex_string(&palw_kernel_bond_id_v1(&producer)));
        owned_dir(&parent)?;
        let lock = parent.join("producer.lock");
        if lock.exists() {
            regular(&lock)?;
        }
        let lock = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(lock).map_err(|e| e.to_string())?;
        lock.try_lock().map_err(|e| format!("V5 original producer/WAL already owned: {e}"))?;
        let old = parent.join(&self.base_claim);
        let mut requests = 0;
        let generation = decode::<Generation>(&self.expected_generation_path, &self, &mut requests)?;
        if generation.root != hash(&self.base_claim)?.as_bytes() {
            return Err("V5 generation names another base claim".into());
        }
        let original = decode::<Object>(&old.join("object.borsh"), &self, &mut requests)?;
        // An existing wrapper is never accepted as the original. Validate the
        // exact signed original namespace before creating any continuation WAL.
        check_workspace(&self, requests, requests.checked_mul(2).ok_or("original namespace decode overlap overflow")?)?;
        let decoded = decode_original(&original, producer)?;
        let (enrollment, _, _, family) = original_parts(&decoded)?;
        self.validate_family(family)?;
        generation.validate(enrollment)?;
        if let OriginalFamily::ApprovedV7(source_policy) = family {
            let lock = crate::palw_producer::parse_outpoint(
                original_config
                    .source_lock_funding_outpoint
                    .as_deref()
                    .ok_or("V7 continuation requires the original explicit source-lock input and fee authority")?,
            )?;
            if lock == producer.0
                || kaspa_consensus_core::config::drill::palw_t12_public_genesis_txid_v1(&lock.transaction_id)
                || original_config
                    .funding_outpoints
                    .iter()
                    .any(|point| crate::palw_producer::parse_outpoint(point).is_ok_and(|p| p == lock))
            {
                return Err("original V7 source-lock input aliases producer/claim funding".into());
            }
            let fence = ctx.config.params.palw_paid_bootstrap_source_v6.ok_or("V7 requires explicit approved source policy")?;
            if source_policy != fence.policy.id() {
                return Err("V7 original source policy differs from configured namespace".into());
            }
        }
        drop(decoded);
        let old_transaction = if old.join("transaction.borsh").exists() {
            let (bytes, _raw) = read_bounded(&old.join("transaction.borsh"), remaining(&self, requests)?.min(self.max_input_bytes))?;
            let quote = carrier_quote(bytes.len() as u64)?;
            check_workspace(&self, requests, quote)?;
            let (tx, owner) = super::recovery_ack_carrier_v4::decode_carrier(&bytes, &original)?;
            if tx.inputs.len() != 1
                || tx.inputs[0].previous_outpoint == funding
                || !original_config.funding_outpoints.iter().any(|point| {
                    crate::palw_producer::parse_outpoint(point).is_ok_and(|point| point == tx.inputs[0].previous_outpoint)
                })
            {
                return Err("V5 original transaction/funding relation differs".into());
            }
            requests = requests.checked_add(quote).ok_or("V5 retained carrier workspace overflow")?;
            Some((tx, owner))
        } else {
            None
        };
        // The original object may coexist with its decoded kernel payload and
        // a second signature-validation/builder decode. Admit all three before
        // retaining the worker, rather than equating encoded bytes with heap.
        let transient = requests.checked_mul(2).ok_or("V5 original decode overlap overflow")?;
        check_workspace(&self, requests, transient)?;
        requests = requests.checked_add(transient).ok_or("V5 aggregate original workspace overflow")?;
        let outbox = old.join(format!("material-generation-v{}-{}", self.version, generation.sequence));
        owned_dir(&outbox)?;
        Ok(PreparedKernelPaidMaterialGenerationV5 {
            spec: self,
            manager: ctx.consensus_manager.clone(),
            flow: ctx.flow_context.clone(),
            tick: ctx.tick_service.clone(),
            config: ctx.config.clone(),
            producer,
            key,
            payer,
            old_payer,
            funding,
            original_config,
            original_outbox: old,
            original,
            old_transaction,
            input_requests: requests,
            generation: *generation,
            outbox,
            _lock: lock,
            _prepare: prepare,
            _original_config_owner: original_config_owner,
        })
    }
}
fn decode<T: NativeWireScanV1 + borsh::BorshDeserialize>(
    path: &Path,
    spec: &PrivateKernelPaidMaterialGenerationSpecV5,
    requests: &mut u64,
) -> Result<NativeRetainedV1<T>, String> {
    let cap = spec
        .max_workspace_bytes
        .checked_sub(*requests)
        .and_then(|n| n.checked_sub(1))
        .ok_or("V5 input has no raw overlap headroom")?
        .min(spec.max_input_bytes);
    let (raw, _raw) = read_bounded(path, cap)?;
    let shape = preflight_native_value_v1::<T>(&raw[..], raw.len() as u64).map_err(|e| e.to_string())?;
    *requests = requests.checked_add(shape.logical_requests_v1()).ok_or("V5 input workspace overflow")?;
    if requests.checked_add(raw.len() as u64 + 1).is_none_or(|n| n > spec.max_workspace_bytes) {
        return Err("V5 aggregate input/decode overlap exceeds explicit workspace".into());
    }
    retain_native_value_decode_v1(&raw, || borsh::from_slice::<T>(&raw)).map_err(|e| e.to_string())
}

fn remaining(spec: &PrivateKernelPaidMaterialGenerationSpecV5, held: u64) -> Result<u64, String> {
    spec.max_workspace_bytes.checked_sub(held).and_then(|n| n.checked_sub(1)).ok_or("V5 explicit workspace exhausted".into())
}
fn check_workspace(spec: &PrivateKernelPaidMaterialGenerationSpecV5, held: u64, additional: u64) -> Result<(), String> {
    if held.checked_add(additional).is_none_or(|n| n > spec.max_workspace_bytes) {
        return Err("V5 simultaneous retained/decode/signing workspace exceeds explicit cap".into());
    }
    Ok(())
}
// The fixed carrier grammar has one input/output and three bounded byte
// vectors. Four wire extents plus fixed layouts conservatively cover raw,
// decoded, payload comparison and relay copies; decode_carrier also acquires
// its actual separate native tickets before those allocations.
fn carrier_quote(bytes: u64) -> Result<u64, String> {
    bytes.checked_mul(4).and_then(|n| n.checked_add(65536)).ok_or("V5 carrier workspace overflow".into())
}

pub struct PreparedKernelPaidMaterialGenerationV5 {
    spec: PrivateKernelPaidMaterialGenerationSpecV5,
    manager: Arc<kaspa_consensusmanager::ConsensusManager>,
    flow: Arc<kaspa_p2p_flows::flow_context::FlowContext>,
    tick: Arc<kaspa_core::task::tick::TickService>,
    config: Arc<kaspa_consensus_core::config::Config>,
    producer: PalwBondKeyV2,
    key: kaspa_pq_validator_core::ValidatorKey,
    payer: libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair,
    old_payer: libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair,
    funding: TransactionOutpoint,
    original_config: OriginalFunding,
    original_outbox: PathBuf,
    original: NativeRetainedV1<Object>,
    old_transaction: Option<(Transaction, Option<NativeDemandLeaseV1>)>,
    input_requests: u64,
    generation: Generation,
    outbox: PathBuf,
    _lock: fs::File,
    _prepare: Option<NativeDemandLeaseV1>,
    _original_config_owner: Option<NativeDemandLeaseV1>,
}
impl PreparedKernelPaidMaterialGenerationV5 {
    pub fn start(self) -> Result<(), String> {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
        let version = self.spec.version;
        std::thread::Builder::new()
            .name(format!("private-paid-material-generation-v{version}"))
            .spawn(move || {
                if let Err(error) = runtime.block_on(self.run()) {
                    kaspa_core::warn!("private material continuation v{version} stopped; WAL retained: {error}");
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    async fn run(self) -> Result<(), String> {
        let object_path = self.outbox.join("object.borsh");
        let transaction_path = self.outbox.join("transaction.borsh");
        let mut requests = self.input_requests;
        let mut object = if object_path.exists() { Some(decode::<Object>(&object_path, &self.spec, &mut requests)?) } else { None };
        let mut transaction = if transaction_path.exists() {
            let saved = object.as_ref().ok_or("V5 transaction lacks signed object")?;
            let (bytes, _raw) = read_bounded(&transaction_path, remaining(&self.spec, requests)?.min(self.spec.max_input_bytes))?;
            let quote = carrier_quote(bytes.len() as u64)?;
            check_workspace(&self.spec, requests, quote)?;
            requests = requests.checked_add(quote).ok_or("V5 retained carrier workspace overflow")?;
            let decoded = super::recovery_ack_carrier_v4::decode_carrier(&bytes, saved)?;
            if decoded.0.inputs.len() != 1 || decoded.0.inputs[0].previous_outpoint != self.funding {
                return Err("V5 cold WAL substitutes the separately authorized input; all WAL retained".into());
            }
            Some(decoded)
        } else {
            None
        };
        loop {
            if matches!(self.tick.tick(Duration::from_secs(2)).await, kaspa_core::task::tick::TickReason::Shutdown) {
                return Ok(());
            }
            let instance = self.manager.consensus();
            let api = instance.unguarded_session_blocking();
            let own = if self.spec.version == 7 {
                let Some(fence) = self.config.params.palw_paid_bootstrap_source_v6 else {
                    return Err("V7 explicit source policy absent".into());
                };
                if !fence.activation.is_active(api.get_virtual_daa_score()) {
                    continue;
                }
                let own = match api.palw_source_bootstrap_epoch_observation_v1() {
                    Ok(own) => own,
                    Err(error) => {
                        kaspa_core::debug!("V7 own source authority unavailable; no signature/funding: {error}");
                        continue;
                    }
                };
                if own.state.bond(&self.producer).is_none_or(|bond| {
                    bond.pubkey.as_slice() != self.key.public_key()
                        || !matches!(bond.status, kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Active)
                }) {
                    kaspa_core::debug!("V7 own producer key/active bond unavailable; no signature/funding");
                    continue;
                }
                let epoch = match own.state.source_bootstrap_epoch_v1() {
                    Ok(Some(epoch)) => epoch,
                    Ok(None) => continue,
                    Err(error) => {
                        kaspa_core::debug!("V7 own source epoch unavailable: {error}");
                        continue;
                    }
                };
                if epoch.draw_refused
                    || fence.policy.bootstrap_policy != epoch.context.policy.id().as_bytes()
                    || fence.policy.network != epoch.context.policy.network.as_bytes()
                {
                    continue;
                }
                drop(epoch);
                Some(own)
            } else {
                None
            };
            let legacy_route = if own.is_none() { api.palw_kernel_route_v1() } else { None };
            let Some(route) = own.as_ref().and_then(|own| own.state.kernel_route()).or(legacy_route.as_ref()) else { continue };
            self.validate_original(route)?;
            if let Some(saved) = &object {
                self.validate_wrapper(&route, saved)?;
            }
            match route.paid_claim_for_base_v2(&self.generation.root) {
                Ok(Some(authority)) => {
                    require_same_accepted_enrollment_v5(&self.generation, authority.enrollment.id())?;
                    // The existing enrollment keeps its fixed native source
                    // binding/draw. Finish only its original fundless Lock action
                    // under this SAME producer.lock and immutable WAL namespace.
                    if self.spec.version == 7 {
                        let point = crate::palw_producer::parse_outpoint(
                            self.original_config
                                .source_lock_funding_outpoint
                                .as_deref()
                                .ok_or("original V7 source-lock funding authority absent")?,
                        )?;
                        super::kernel_paid_producer_v4::drive_paid_source_lock_v6(
                            super::kernel_paid_producer_v4::PaidSourceLockContextV6 {
                                manager: &self.manager,
                                flow: &self.flow,
                                config: &self.config,
                                producer: self.producer,
                                key: &self.key,
                                payer: &self.old_payer,
                                funding: point,
                                maximum_fee_sompi: self
                                    .original_config
                                    .maximum_source_lock_fee_sompi
                                    .ok_or("original V7 source-lock fee authority absent")?,
                                maximum_input_bytes: self.spec.max_input_bytes.min(self.original_config.max_input_bytes),
                                maximum_workspace_bytes: remaining(&self.spec, requests)?
                                    .min(self.original_config.max_workspace_bytes),
                            },
                            route,
                            &authority,
                            &self.original_outbox,
                        )
                        .await?;
                    }
                    // Acceptance is reversible. Keep observing; never create
                    // another enrollment, draw, replacement fee or receipt.
                    continue;
                }
                Err(_) => continue,
                Ok(None) => {}
            }
            // A rooted lock/binding without readable enrollment is UNKNOWN,
            // never permission to recreate a draw or to fund another wrapper.
            if route.aux.iter().any(|((table, key), _)| {
                *table == kaspa_consensus_core::palw_claim_challenge_v2::PAID_CLAIM_LOCKS_V2
                    && key.as_slice() == self.generation.enrollment.as_slice()
            }) {
                continue;
            }
            if self.spec.version == 7 {
                match (
                    route.paid_bootstrap_binding_v6(&self.generation.enrollment),
                    route.paid_bootstrap_lock_v6(&self.generation.enrollment),
                ) {
                    (Ok(None), Ok(None)) => {}
                    _ => {
                        kaspa_core::debug!("V7 own accepted source binding/lock or unavailable metadata; monitor original WAL");
                        continue;
                    }
                }
            }
            let old_inner = self.original_inner()?;
            let (enrollment, commit, _, family) = original_parts(&old_inner)?;
            let now = own.as_ref().map_or_else(|| api.get_virtual_daa_score(), |own| own.virtual_daa);
            if now < enrollment.lifetime.committed_daa || now > enrollment.accepted_until_daa {
                continue;
            }
            if matches!(family, OriginalFamily::ApprovedV7(_)) {
                match approved_scope_from_route(route, enrollment, commit, now, remaining(&self.spec, requests)?) {
                    Ok(true) => {}
                    Ok(false) => continue,
                    Err(error) => {
                        kaspa_core::debug!("V7 own scope/lifetime unobserved; no new signature/funding: {error}");
                        continue;
                    }
                }
            }
            let initial = match initial_generation_from_route(
                route,
                &self.generation,
                self.producer,
                &self.config,
                now,
                remaining(&self.spec, requests)?,
            ) {
                Ok(initial) => initial,
                Err(error) => {
                    kaspa_core::debug!("continuation active material/recovery unobserved; no signature/funding: {error}");
                    continue;
                }
            };
            let (current, initial_current) = match paid_material_generation_from_route_v5(route, enrollment, self.producer) {
                Ok(current) => (Some(current), false),
                Err(NativeError::Resource(error)) => {
                    kaspa_core::debug!("continuation native generation resource unavailable: {error}");
                    continue;
                }
                Err(NativeError::Decode(_)) => {
                    // Only exact own initial metadata allows old-transaction
                    // replay. Corrupt/absent/terminal/unknown history is a hold.
                    (None, initial)
                }
            };
            let selection = select_exact_generation_v5(&self.generation, current.as_ref(), initial_current);
            if selection == ResumeV5::Expected {
                if object.is_none() {
                    // Admit the old decode / wrapper / signing overlap before
                    // calling a builder that moves the original commitment.
                    check_workspace(
                        &self.spec,
                        requests,
                        carrier_quote(borsh::object_length(&*self.original).map_err(|e| e.to_string())? as u64)?,
                    )?;
                    let built = match family {
                        OriginalFamily::LegacyV5 => {
                            misaka_palw_sdk::claim_paid_v2::material_generation_v5::paid_claim_material_generation_object_v5(
                                route,
                                &self.original,
                                self.key.public_key(),
                            )
                        }
                        OriginalFamily::ApprovedV7(_) => {
                            misaka_palw_sdk::claim_paid_v2::material_generation_v7::paid_claim_material_generation_object_v7(
                                route,
                                &self.original,
                                self.key.public_key(),
                            )
                        }
                    };
                    let inner = match built {
                        Ok(inner) => inner,
                        Err(error) => {
                            kaspa_core::debug!("continuation builder unavailable; no new signature/funding: {error}");
                            continue;
                        }
                    };
                    let signing = (borsh::object_length(&*inner).map_err(|e| e.to_string())? as u64)
                        .checked_mul(4)
                        .and_then(|n| n.checked_add(65536))
                        .ok_or("V5 signing workspace overflow")?;
                    check_workspace(&self.spec, requests, signing)?;
                    let _signing = reserve_native_requests_v1(signing).map_err(|e| e.to_string())?;
                    let signed = misaka_palw_sdk::claim_paid_v2::sign_paid_claim_object_v2(
                        &self.key,
                        Hash64::from_bytes(route.header.policy.network_domain),
                        self.producer,
                        &inner,
                    )?;
                    let signed = NativeRetainedV1::retain_existing_v1(signed).map_err(|e| e.to_string())?;
                    self.validate_wrapper(&route, &signed)?;
                    publish_borsh(&object_path, &*signed, self.spec.max_input_bytes)?;
                    requests = requests.checked_add(signing).ok_or("V5 retained wrapper workspace overflow")?;
                    object = Some(signed);
                }
                let signed = object.as_ref().ok_or("V5 signed object absent")?;
                if !matches!(api.palw_object_rehearsal_v1(signed), Some(PalwObjectRehearsalV1::Accepted)) {
                    continue;
                }
                if transaction.is_none() {
                    let daa = api.get_virtual_daa_score();
                    let Some(entry) = api.get_virtual_utxo_entry(self.funding) else { continue };
                    let minimum =
                        crate::palw_panel::palw_carrier_fees::palw_lifecycle_funding_minimum_v2(&self.config.params, signed, daa);
                    if entry.amount < minimum || !self.usable(&api, self.funding, &entry, &self.payer, daa) {
                        continue;
                    }
                    let quote = carrier_quote(borsh::object_length(&**signed).map_err(|e| e.to_string())? as u64)?;
                    check_workspace(&self.spec, requests, quote)?;
                    let owner = super::recovery_ack_carrier_v4::reserve_carrier_build_v4(
                        signed,
                        &entry,
                        self.payer.verification_key.as_ref().len(),
                    )?;
                    let tx = crate::palw_panel::palw_carrier_fees::build_lifecycle_carrier_v2(
                        &self.config,
                        &self.payer,
                        signed,
                        self.funding,
                        &entry,
                        &[],
                        None,
                        daa,
                    )?;
                    check_fee(&tx, &entry, self.spec.maximum_fee_sompi)?;
                    publish_borsh(&transaction_path, &tx, self.spec.max_input_bytes)?;
                    transaction = Some((tx, owner));
                    requests = requests.checked_add(quote).ok_or("V5 retained new carrier workspace overflow")?;
                }
                let (tx, _owner) = transaction.as_ref().unwrap();
                if tx.inputs.len() != 1 || tx.inputs[0].previous_outpoint != self.funding {
                    return Err("V5 WAL substitutes its separately authorized funding slot".into());
                }
                self.relay(signed, tx, &self.payer, self.spec.maximum_fee_sompi).await?;
            } else if selection == ResumeV5::Original {
                // Rollback to the original generation resumes ONLY its exact
                // prior signed transaction. Missing old tx means hold, not an
                // additional unrequested spend/reconstruction from new inputs.
                if let Some((tx, _owner)) = &self.old_transaction {
                    if matches!(api.palw_object_rehearsal_v1(&self.original), Some(PalwObjectRehearsalV1::Accepted)) {
                        self.relay(&self.original, tx, &self.old_payer, self.original_config.maximum_fee_sompi).await?;
                    }
                }
            }
            // Other/unknown/terminal generations retain both exact WALs.
        }
    }
    fn original_inner(&self) -> Result<NativeRetainedV1<Kernel>, String> {
        decode_original(&self.original, self.producer)
    }
    fn validate_original(&self, route: &PalwKernelRouteStateV1) -> Result<(), String> {
        let inner = self.original_inner()?;
        let (enrollment, commit, salt, family) = original_parts(&inner)?;
        self.spec.validate_family(family)?;
        self.generation.validate(enrollment)?;
        if let OriginalFamily::ApprovedV7(policy) = family {
            if self.config.params.palw_paid_bootstrap_source_v6.map(|f| f.policy.id()) != Some(policy) {
                return Err("V7 original source policy differs from explicit compiled policy".into());
            }
        }
        let Object::KernelRouteV1 { bytes, signature, .. } = &*self.original else { unreachable!() };
        let network = Hash64::from_bytes(route.header.policy.network_domain);
        let message = original_message(network, &self.producer, enrollment, commit, salt, family)?;
        let (base_claim, _, commit_producer) = kaspa_consensus_core::palw_prospective_kernel_material_v4::commit_identity_v4(commit);
        if enrollment.producer != palw_kernel_bond_id_v1(&self.producer)
            || commit_producer != enrollment.producer
            || base_claim != enrollment.base_claim
            || message != palw_kernel_route_message_v1(network, &self.producer, bytes)
            || !kaspa_consensus_core::palw_attempt_material_v3::verify_ready_signature_v3(
                self.key.public_key(),
                message.as_byte_slice(),
                signature,
                PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1,
            )
        {
            return Err("continuation original signed identity/network/source policy differs".into());
        }
        Ok(())
    }
    fn validate_wrapper(&self, route: &PalwKernelRouteStateV1, object: &Object) -> Result<(), String> {
        let Object::KernelRouteV1 { bytes, signer, signature } = object else {
            return Err("continuation wrapper carrier kind differs".into());
        };
        let original = self.original_inner()?;
        let (_, _, _, family) = original_parts(&original)?;
        self.spec.validate_family(family)?;
        let raw = bytes.get(1..).ok_or("wrapper route version absent")?;
        preflight_native_value_v1::<Kernel>(raw, raw.len() as u64).map_err(|e| e.to_string())?;
        let inner = retain_native_value_decode_v1(raw, || Kernel::decode(bytes)).map_err(|e| e.to_string())?;
        let (enrollment, commit, salt, generation, original_signature) = match (&*inner, family) {
            (
                Kernel::PaidClaimV2 {
                    object: Paid::CommitWithMaterialGenerationV5 { enrollment, commit, salt, generation, original_signature },
                },
                OriginalFamily::LegacyV5,
            ) => (enrollment, commit, salt, generation, original_signature),
            (
                Kernel::PaidClaimV2 {
                    object:
                        Paid::CommitApprovedBootstrapWithMaterialGenerationV7 {
                            enrollment,
                            commit,
                            salt,
                            source_policy,
                            generation,
                            original_signature,
                        },
                },
                OriginalFamily::ApprovedV7(expected),
            ) if *source_policy == expected => (enrollment, commit, salt, generation, original_signature),
            _ => return Err("continuation wrapper changes the exact original namespace/source policy".into()),
        };
        let Object::KernelRouteV1 { bytes: old_bytes, signature: old_signature, .. } = &*self.original else { unreachable!() };
        let network = Hash64::from_bytes(route.header.policy.network_domain);
        generation.validate(enrollment)?;
        if *signer != self.producer
            || *generation != self.generation
            || original_signature != old_signature
            || original_message(network, signer, enrollment, commit, salt, family)?
                != palw_kernel_route_message_v1(network, signer, old_bytes)
            || !kaspa_consensus_core::palw_attempt_material_v3::verify_ready_signature_v3(
                self.key.public_key(),
                palw_kernel_route_message_v1(network, signer, bytes).as_byte_slice(),
                signature,
                PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1,
            )
        {
            return Err("continuation immutable wrapper changes old signature/work/generation/actor".into());
        }
        Ok(())
    }
    fn usable(
        &self,
        api: &kaspa_consensusmanager::ConsensusSessionBlocking<'_>,
        point: TransactionOutpoint,
        entry: &kaspa_consensus_core::tx::UtxoEntry,
        payer: &libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair,
        daa: u64,
    ) -> bool {
        let script = kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk(
            &kaspa_hashes::blake2b_512_address_payload(payer.verification_key.as_ref()).as_bytes(),
        );
        entry.script_public_key == script
            && api.palw_bond_payout_payload_v2(PalwBondKeyV2(point)).is_none()
            && !self.flow.mining_manager().outpoint_is_spent_in_mempool(&point)
            && crate::palw_panel::palw_fee_funding_usable_v1(
                &point,
                entry,
                daa,
                self.config.params.coinbase_spend_maturity(),
                None,
                &std::collections::HashSet::new(),
            )
    }
    async fn relay(
        &self,
        signed: &Object,
        tx: &Transaction,
        payer: &libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair,
        cap: u64,
    ) -> Result<(), String> {
        if tx.inputs.len() != 1 {
            return Err("V5 prior/current carrier is not a single declared input".into());
        }
        let instance = self.manager.consensus();
        let api = instance.unguarded_session_blocking();
        let point = tx.inputs[0].previous_outpoint;
        let Some(entry) = api.get_virtual_utxo_entry(point) else { return Ok(()) };
        if !self.usable(&api, point, &entry, payer, api.get_virtual_daa_score()) {
            return Ok(());
        }
        check_fee(tx, &entry, cap)?;
        let _relay = super::recovery_ack_carrier_v4::reserve_carrier_build_v4(signed, &entry, payer.verification_key.as_ref().len())?;
        match tokio::time::timeout(
            Duration::from_secs(10),
            self.flow.submit_rpc_transaction(&instance.unguarded_session(), tx.clone(), kaspa_mining::mempool::tx::Orphan::Forbidden),
        )
        .await
        {
            Ok(Ok(())) => {}
            Ok(Err(error)) => kaspa_core::debug!("V5 exact durable relay pending: {error}"),
            Err(_) => kaspa_core::debug!("V5 exact durable relay timed out"),
        }
        Ok(())
    }
}

/// Read the actual own class/job before signing a V7 continuation. Unsupported
/// scopes and stale finite admission windows cannot create an outer signature.
/// Missing/oversized history remains typed unavailability, never legacy consent.
fn approved_scope_from_route(
    route: &PalwKernelRouteStateV1,
    enrollment: &misaka_palw_kernel::claim_paid_v2::PaidClaimEnrollmentV2,
    commit: &misaka_palw_kernel::route::SaltedCommitV1,
    now: u64,
    maximum_workspace: u64,
) -> Result<bool, NativeError<String>> {
    use kaspa_consensus_core::{
        palw_claim_challenge_v2::{PAID_CLAIM_CLASSES_V2, PAID_CLAIM_JOBS_V2},
        palw_native_resource_v1::NativeCoverageV1,
    };
    use misaka_palw_kernel::claim_paid_v2::{PaidClaimClassV2, PaidClaimJobV2, PaidClaimLifetimeV2};
    let raw = |table, id: &[u8; 64]| {
        route
            .aux
            .iter()
            .find_map(|((t, key), bytes)| (*t == table && key.as_slice() == id.as_slice()).then_some(bytes.as_slice()))
            .ok_or_else(|| NativeError::Decode("own V7 class/job absent".into()))
    };
    let class_raw = raw(PAID_CLAIM_CLASSES_V2, &enrollment.class)?;
    let job_raw = raw(PAID_CLAIM_JOBS_V2, &enrollment.job)?;
    let shape = |error: std::io::Error| {
        NativeError::Resource(NativeCoverageV1::unsupported::<PaidClaimClassV2>("V7 own class/job shape", &error.to_string()))
    };
    let class_quote = preflight_native_value_v1::<PaidClaimClassV2>(class_raw, class_raw.len() as u64).map_err(shape)?;
    let job_quote = preflight_native_value_v1::<PaidClaimJobV2>(job_raw, job_raw.len() as u64).map_err(shape)?;
    let requests = class_quote
        .logical_requests_v1()
        .checked_add(job_quote.logical_requests_v1())
        .ok_or_else(|| NativeError::Resource(NativeCoverageV1::overflow::<PaidClaimClassV2>("V7 class/job overlap")))?;
    if requests > maximum_workspace {
        return Err(NativeError::Resource(NativeCoverageV1::unsupported::<PaidClaimClassV2>(
            "V7 class/job overlap",
            "explicit workspace ceiling exhausted",
        )));
    }
    let class = retain_native_value_decode_classified_v1(class_raw, || {
        borsh::from_slice::<PaidClaimClassV2>(class_raw).map_err(|e| e.to_string())
    })?;
    let job =
        retain_native_value_decode_classified_v1(job_raw, || borsh::from_slice::<PaidClaimJobV2>(job_raw).map_err(|e| e.to_string()))?;
    class.validate().map_err(NativeError::Decode)?;
    let (root, base_job, producer) = kaspa_consensus_core::palw_prospective_kernel_material_v4::commit_identity_v4(commit);
    let lifetime = PaidClaimLifetimeV2::of(&class, enrollment.lifetime.committed_daa).map_err(NativeError::Decode)?;
    let accepted_until = enrollment.lifetime.committed_daa.checked_add(class.maximum_commit_delay_daa);
    Ok(class.id() == enrollment.class
        && job.id() == enrollment.job
        && job.class == enrollment.class
        && job.version == 2
        && job.maximum_consumers > 0
        && job.base_job == base_job
        && root == enrollment.base_claim
        && producer == enrollment.producer
        && class.supports_approved_bootstrap_source_v6()
        && lifetime == enrollment.lifetime
        && accepted_until == Some(enrollment.accepted_until_daa)
        && now >= enrollment.lifetime.committed_daa
        && now <= enrollment.accepted_until_daa
        && now <= job.last_commit_daa)
}

/// Only an exact active initial contract on this SAME native route permits
/// replaying the old original transaction. No failed current-generation read,
/// elapsed local timer or monetary consent alone supplies that authority.
fn initial_generation_from_route(
    route: &PalwKernelRouteStateV1,
    expected: &Generation,
    producer: PalwBondKeyV2,
    config: &kaspa_consensus_core::config::Config,
    now: u64,
    maximum_workspace: u64,
) -> Result<bool, NativeError<String>> {
    use kaspa_consensus_core::{
        palw_material_release_recovery_v2::{PALW_MATERIAL_RELEASE_RECOVERY_TABLE_V2, PalwMaterialRecoveryRowV2},
        palw_monetary_material_v1::{PALW_MONETARY_MATERIAL_TABLE_V1, PalwMonetaryMaterialRowV1},
        palw_native_resource_v1::NativeCoverageV1,
    };
    fn raw<'a>(route: &'a PalwKernelRouteStateV1, table: u8, root: &[u8; 64]) -> Result<&'a [u8], NativeError<String>> {
        route
            .aux
            .iter()
            .find_map(|((t, key), raw)| (*t == table && key.as_slice() == root.as_slice()).then_some(raw.as_slice()))
            .ok_or_else(|| NativeError::Decode("own initial material history absent".into()))
    }
    let monetary_raw = raw(route, PALW_MONETARY_MATERIAL_TABLE_V1, &expected.root)?;
    let recovery_raw = raw(route, PALW_MATERIAL_RELEASE_RECOVERY_TABLE_V2, &expected.root)?;
    let shape = |error: std::io::Error| {
        NativeError::Resource(NativeCoverageV1::unsupported::<Generation>("initial generation shape", &error.to_string()))
    };
    let monetary_quote =
        preflight_native_value_v1::<PalwMonetaryMaterialRowV1>(monetary_raw, monetary_raw.len() as u64).map_err(shape)?;
    let recovery_quote =
        preflight_native_value_v1::<PalwMaterialRecoveryRowV2>(recovery_raw, recovery_raw.len() as u64).map_err(shape)?;
    let requests = monetary_quote
        .logical_requests_v1()
        .checked_add(recovery_quote.logical_requests_v1())
        .ok_or_else(|| NativeError::Resource(NativeCoverageV1::overflow::<Generation>("initial metadata overlap")))?;
    if requests > maximum_workspace {
        return Err(NativeError::Resource(NativeCoverageV1::unsupported::<Generation>(
            "initial metadata overlap",
            "explicit workspace ceiling exhausted",
        )));
    }
    let monetary = retain_native_value_decode_classified_v1(monetary_raw, || {
        borsh::from_slice::<PalwMonetaryMaterialRowV1>(monetary_raw).map_err(|e| e.to_string())
    })?;
    let recovery = retain_native_value_decode_classified_v1(recovery_raw, || {
        borsh::from_slice::<PalwMaterialRecoveryRowV2>(recovery_raw).map_err(|e| e.to_string())
    })?;
    let Some(policy) = config.params.palw_material_release_recovery_v2.as_ref().filter(|f| f.activation.is_active(now)) else {
        return Err(NativeError::Decode("material recovery policy is inactive".into()));
    };
    recovery.validate(&policy.policy).map_err(|e| NativeError::Decode(e.to_owned()))?;
    let PalwMaterialRecoveryRowV2::Active(contract) = &*recovery else {
        return Err(NativeError::Decode("material recovery is terminal".into()));
    };
    if now > monetary.terms.enrollment_until_daa {
        return Err(NativeError::Decode("material enrollment lifetime elapsed".into()));
    }
    Ok(monetary.sequence == 0
        && monetary.consent.as_bytes() == expected.initial_consent
        && monetary.terms.root.as_bytes() == expected.root
        && monetary.terms.sponsor == producer
        && monetary.terms.native_session.is_none()
        && now <= monetary.terms.enrollment_until_daa
        && monetary.terms.material_manifest.as_ref().is_some_and(|m| m.root.as_bytes() == expected.root)
        && contract.id().as_bytes() == expected.recovery
        && contract.terms.root.as_bytes() == expected.root
        && contract.terms.sponsor == producer
        && contract.terms.nonce == monetary.terms.nonce
        && contract.terms.initial_consent.as_bytes() == expected.initial_consent
        && contract.generations.len() == 1
        && contract.generations[0].consent == monetary.consent)
}

#[derive(Debug, PartialEq, Eq)]
enum ResumeV5 {
    Expected,
    Original,
    Hold,
}
fn select_exact_generation_v5(expected: &Generation, current: Option<&Generation>, initial_current: bool) -> ResumeV5 {
    if current == Some(expected) {
        ResumeV5::Expected
    } else if initial_current {
        ResumeV5::Original
    } else {
        ResumeV5::Hold
    }
}
fn require_same_accepted_enrollment_v5(expected: &Generation, accepted: [u8; 64]) -> Result<(), String> {
    if accepted != expected.enrollment {
        return Err("V5 existing base has another enrollment; no reroll or new funding permitted".into());
    }
    Ok(())
}

#[cfg(test)]
mod source_controls {
    use super::*;
    #[test]
    fn rollback_and_forward_resume_exact_wals_without_rebinding_or_a_second_enrollment() {
        let expected = Generation {
            version: 5,
            enrollment: [1; 64],
            root: [2; 64],
            initial_consent: [3; 64],
            recovery: [4; 64],
            current_consent: [5; 64],
            sequence: 1,
        };
        assert_eq!(select_exact_generation_v5(&expected, Some(&expected), false), ResumeV5::Expected);
        assert_eq!(select_exact_generation_v5(&expected, None, true), ResumeV5::Original);
        assert_eq!(select_exact_generation_v5(&expected, None, false), ResumeV5::Hold);
        let later = Generation { current_consent: [6; 64], sequence: 2, ..expected };
        assert_eq!(select_exact_generation_v5(&expected, Some(&later), false), ResumeV5::Hold);
        assert_eq!(select_exact_generation_v5(&expected, Some(&expected), false), ResumeV5::Expected);
        assert!(require_same_accepted_enrollment_v5(&expected, expected.enrollment).is_ok());
        assert!(
            require_same_accepted_enrollment_v5(&expected, [9; 64]).is_err(),
            "same base cannot silently acquire another enrollment"
        );
    }
    #[test]
    fn original_namespace_and_policy_cannot_be_relabelled_or_nested() {
        use misaka_palw_kernel::{
            claim_paid_v2::{PaidClaimEnrollmentV2, PaidClaimLifetimeV2},
            route::SaltedCommitV1,
            spec::{SpecClaimV1, retrieval::RetrievalClaimV1},
        };
        // Codec/identity control only: this empty retrieval is not native work.
        let commit = SaltedCommitV1::Spec {
            claim: SpecClaimV1::Retrieval(RetrievalClaimV1 { job_id: [1; 64], producer_bond: [2; 64], result: vec![] }),
        };
        let enrollment = PaidClaimEnrollmentV2 {
            version: 2,
            class: [3; 64],
            job: [4; 64],
            base_claim: [5; 64],
            producer: [2; 64],
            commit_root: [6; 64],
            lifetime: PaidClaimLifetimeV2 {
                committed_daa: 1,
                checking_until: 2,
                challenge_until: 3,
                dispute_until: 4,
                liability_until: 5,
            },
            accepted_until_daa: 2,
            material_consent: [7; 64],
            round: 0,
            prior_transcript: [0; 64],
        };
        let old =
            Kernel::PaidClaimV2 { object: Paid::Commit { enrollment: enrollment.clone(), commit: commit.clone(), salt: [8; 64] } };
        let approved = Kernel::PaidClaimV2 {
            object: Paid::CommitWithApprovedBootstrapV6 {
                enrollment: enrollment.clone(),
                commit: commit.clone(),
                salt: [8; 64],
                source_policy: [9; 64],
            },
        };
        assert_eq!(original_parts(&old).unwrap().3, OriginalFamily::LegacyV5);
        assert_eq!(original_parts(&approved).unwrap().3, OriginalFamily::ApprovedV7([9; 64]));
        let actor = PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_bytes([10; 64]), 0));
        let legacy =
            original_message(Hash64::from_bytes([11; 64]), &actor, &enrollment, &commit, &[8; 64], OriginalFamily::LegacyV5).unwrap();
        let v7 = original_message(
            Hash64::from_bytes([11; 64]),
            &actor,
            &enrollment,
            &commit,
            &[8; 64],
            OriginalFamily::ApprovedV7([9; 64]),
        )
        .unwrap();
        assert_ne!(legacy, v7, "V6 policy identity is part of the exact original signature message");
        let generation = Generation {
            version: 5,
            enrollment: enrollment.id(),
            root: enrollment.base_claim,
            initial_consent: enrollment.material_consent,
            recovery: [12; 64],
            current_consent: [13; 64],
            sequence: 1,
        };
        let wrapper = Kernel::PaidClaimV2 {
            object: Paid::CommitApprovedBootstrapWithMaterialGenerationV7 {
                enrollment,
                commit,
                salt: [8; 64],
                source_policy: [9; 64],
                generation,
                original_signature: vec![14; 8],
            },
        };
        assert!(original_parts(&wrapper).is_err(), "a continuation cannot become another original enrollment");
    }
    #[test]
    fn old_wal_and_exact_workspace_caps_are_preserved() {
        // File/authorization controls only; no node or carrier was run.
        let temp = tempfile::tempdir().unwrap();
        let temp = temp.path().canonicalize().unwrap();
        let old = temp.join("object.borsh");
        publish_borsh(&old, &vec![1u8, 2, 3], 100).unwrap();
        let original = fs::read(&old).unwrap();
        let new = temp.join("material-generation-v5-1");
        owned_dir(&new).unwrap();
        publish_borsh(&new.join("object.borsh"), &vec![4u8, 5, 6], 100).unwrap();
        assert!(publish_borsh(&old, &vec![9u8], 100).is_err());
        assert_eq!(fs::read(&old).unwrap(), original);
        let spec = PrivateKernelPaidMaterialGenerationSpecV5 {
            version: 5,
            base_claim: "00".repeat(64),
            expected_generation_path: old.clone(),
            original_producer_spec_path: old.clone(),
            payer_key: old,
            funding_outpoint: String::new(),
            maximum_fee_sompi: 10,
            maximum_total_additional_fee_sompi: 10,
            max_input_bytes: 100,
            max_workspace_bytes: 1000,
            _owner: None,
        };
        assert!(check_workspace(&spec, 800, 200).is_ok());
        assert!(check_workspace(&spec, 801, 200).is_err());
        assert!(check_workspace(&spec, u64::MAX, 1).is_err());
        assert!(carrier_quote(u64::MAX).is_err());
    }
}
fn check_fee(tx: &Transaction, entry: &kaspa_consensus_core::tx::UtxoEntry, cap: u64) -> Result<(), String> {
    let fee = tx.outputs.iter().try_fold(0u64, |sum, output| sum.checked_add(output.value)).and_then(|n| entry.amount.checked_sub(n));
    if fee.is_none_or(|fee| fee > cap) || tx.outputs.len() != 1 || tx.outputs[0].script_public_key != entry.script_public_key {
        return Err("V5 exact carrier exceeds its fee cap or redirects the authorized payer change".into());
    }
    Ok(())
}
