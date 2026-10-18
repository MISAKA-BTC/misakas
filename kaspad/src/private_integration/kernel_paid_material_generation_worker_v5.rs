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
        NativeDemandLeaseV1, NativeRetainedV1, NativeWireScanV1, preflight_native_value_v1, reserve_native_requests_v1,
        retain_native_value_decode_v1,
    },
    palw_paid_material_generation_v5::{original_paid_commit_message_v5, paid_material_generation_from_route_v5},
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
}
impl PrivateKernelPaidMaterialGenerationSpecV5 {
    pub fn read(path: &Path) -> Result<Self, String> {
        let (bytes, _raw) = read_bounded(path, 1 << 20)?;
        let owner = reserve_native_requests_v1(
            (bytes.len() as u64)
                .checked_mul(8)
                .and_then(|n| n.checked_add(std::mem::size_of::<Self>() as u64))
                .ok_or("V5 config overflow")?,
        )
        .map_err(|e| e.to_string())?;
        let mut spec: Self = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        if spec.version != 5
            || spec.max_input_bytes == 0
            || spec.max_workspace_bytes == 0
            || spec.max_input_bytes > super::native_provider::PRIVATE_NATIVE_EXCHANGE_MAX_FRAME_V3
            || spec.maximum_fee_sompi == 0
            || spec.maximum_fee_sompi > spec.maximum_total_additional_fee_sompi
        {
            return Err("V5 needs exact version, finite input/workspace and explicit one-slot total fee authorization".into());
        }
        hash(&spec.base_claim)?;
        regular(&spec.expected_generation_path)?;
        regular(&spec.original_producer_spec_path)?;
        regular(&spec.payer_key)?;
        crate::palw_producer::parse_outpoint(&spec.funding_outpoint)?;
        spec._owner = owner;
        Ok(spec)
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
        if kaspa_consensus_core::config::drill::palw_t12_public_genesis_txid_v1(&funding.transaction_id) {
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
        if original_config.funding_outpoints.is_empty() || original_config.maximum_fee_sompi == 0 {
            return Err("V5 original funding declaration is empty".into());
        }
        for point in &original_config.funding_outpoints {
            if crate::palw_producer::parse_outpoint(point)? == funding {
                return Err("V5 additional input reuses original funding authorization".into());
            }
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
        let outbox = old.join(format!("material-generation-v5-{}", generation.sequence));
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
    let shape = preflight_native_value_v1::<T>(&raw, raw.len() as u64).map_err(|e| e.to_string())?;
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
        std::thread::Builder::new()
            .name("private-paid-material-generation-v5".into())
            .spawn(move || {
                if let Err(error) = runtime.block_on(self.run()) {
                    kaspa_core::warn!("private V5 continuation stopped; WAL retained: {error}");
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
            let Some(route) = api.palw_kernel_route_v1() else { continue };
            self.validate_original(&route)?;
            if let Some(saved) = &object {
                self.validate_wrapper(&route, saved)?;
            }
            match route.paid_claim_for_base_v2(&self.generation.root) {
                Ok(Some(authority)) => {
                    require_same_accepted_enrollment_v5(&self.generation, authority.enrollment.id())?;
                    // Current acceptance is reversible. Observe forever; never
                    // recreate an accepted source, lock, snapshot or receipt.
                    continue;
                }
                Err(_) => continue,
                Ok(None) => {}
            }
            let old_inner = self.original_inner()?;
            let Kernel::PaidClaimV2 { object: Paid::Commit { enrollment, .. } } = &*old_inner else { unreachable!() };
            let current = paid_material_generation_from_route_v5(&route, enrollment, self.producer);
            let initial_current = if current.as_ref().is_ok_and(|g| *g == self.generation) {
                false
            } else {
                api.palw_kernel_material_consent_accepted_v4(
                    Hash64::from_bytes(self.generation.root),
                    Hash64::from_bytes(self.generation.initial_consent),
                )
                .unwrap_or(false)
            };
            let selection = select_exact_generation_v5(&self.generation, current.as_ref().ok(), initial_current);
            if selection == ResumeV5::Expected {
                if object.is_none() {
                    // Admit the old decode / wrapper / signing overlap before
                    // calling a builder that moves the original commitment.
                    check_workspace(
                        &self.spec,
                        requests,
                        carrier_quote(borsh::object_length(&*self.original).map_err(|e| e.to_string())? as u64)?,
                    )?;
                    let inner = misaka_palw_sdk::claim_paid_v2::material_generation_v5::paid_claim_material_generation_object_v5(
                        &route,
                        &self.original,
                        self.key.public_key(),
                    )?;
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
        let Object::KernelRouteV1 { bytes, signer, .. } = &*self.original else {
            return Err("V5 original carrier kind differs".into());
        };
        if *signer != self.producer {
            return Err("V5 original actor differs".into());
        }
        let raw = bytes.get(1..).ok_or("V5 original kernel version absent")?;
        preflight_native_value_v1::<Kernel>(raw, raw.len() as u64).map_err(|e| e.to_string())?;
        retain_native_value_decode_v1(raw, || Kernel::decode(bytes)).map_err(|e| e.to_string())
    }
    fn validate_original(&self, route: &PalwKernelRouteStateV1) -> Result<(), String> {
        let inner = self.original_inner()?;
        let Kernel::PaidClaimV2 { object: Paid::Commit { enrollment, commit, salt } } = &*inner else {
            return Err("V5 original object is not the exact V2 commit".into());
        };
        self.generation.validate(enrollment)?;
        let Object::KernelRouteV1 { bytes, signature, .. } = &*self.original else { unreachable!() };
        let message = original_paid_commit_message_v5(
            Hash64::from_bytes(route.header.policy.network_domain),
            &self.producer,
            enrollment,
            commit,
            salt,
        )?;
        if message != palw_kernel_route_message_v1(Hash64::from_bytes(route.header.policy.network_domain), &self.producer, bytes)
            || !kaspa_consensus_core::palw_attempt_material_v3::verify_ready_signature_v3(
                self.key.public_key(),
                message.as_byte_slice(),
                signature,
                PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1,
            )
        {
            return Err("V5 original signed identity/network differs".into());
        }
        Ok(())
    }
    fn validate_wrapper(&self, route: &PalwKernelRouteStateV1, object: &Object) -> Result<(), String> {
        let Object::KernelRouteV1 { bytes, signer, signature } = object else {
            return Err("V5 wrapper carrier kind differs".into());
        };
        let raw = bytes.get(1..).ok_or("V5 wrapper route version absent")?;
        preflight_native_value_v1::<Kernel>(raw, raw.len() as u64).map_err(|e| e.to_string())?;
        let inner = retain_native_value_decode_v1(raw, || Kernel::decode(bytes)).map_err(|e| e.to_string())?;
        let Kernel::PaidClaimV2 {
            object: Paid::CommitWithMaterialGenerationV5 { enrollment, commit, salt, generation, original_signature },
        } = &*inner
        else {
            return Err("V5 wrapper WAL changes object kind".into());
        };
        let Object::KernelRouteV1 { bytes: old_bytes, signature: old_signature, .. } = &*self.original else { unreachable!() };
        let network = Hash64::from_bytes(route.header.policy.network_domain);
        if *signer != self.producer
            || *generation != self.generation
            || original_signature != old_signature
            || original_paid_commit_message_v5(network, signer, enrollment, commit, salt)?
                != palw_kernel_route_message_v1(network, signer, old_bytes)
            || !kaspa_consensus_core::palw_attempt_material_v3::verify_ready_signature_v3(
                self.key.public_key(),
                palw_kernel_route_message_v1(network, signer, bytes).as_byte_slice(),
                signature,
                PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1,
            )
        {
            return Err("V5 immutable wrapper changes old signed work/generation/actor".into());
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
