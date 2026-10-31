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
    /// Separate immutable carrier budget for the explicit V6 source lock.
    source_lock_funding_outpoint: Option<String>,
    maximum_source_lock_fee_sompi: Option<u64>,
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
pub(super) fn publish_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
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

/// Recovery artifacts precede the signed claim WAL. A crash between these
/// publications must accept an EXACT existing artifact, never overwrite it or
/// accept a filename alone. Compare the canonical encoding directly against
/// the file with fixed stack scratch; do not duplicate a whole material bundle.
fn publish_artifact_or_match<T: borsh::BorshSerialize>(path: &Path, value: &T, cap: u64) -> Result<(), String> {
    if !path.exists() {
        return publish_borsh(path, value, cap);
    }
    regular(path)?;
    let length = borsh::object_length(value).map_err(|e| e.to_string())? as u64;
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    if length > cap || file.metadata().map_err(|e| e.to_string())?.len() != length {
        return Err("existing kernel recovery artifact extent differs; immutable artifact retained".into());
    }
    struct Compare {
        file: fs::File,
        remaining: u64,
    }
    impl Write for Compare {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() as u64 > self.remaining {
                return Err(std::io::Error::other("kernel recovery artifact encoding grew"));
            }
            let mut scratch = [0u8; 4096];
            for part in bytes.chunks(scratch.len()) {
                self.file.read_exact(&mut scratch[..part.len()])?;
                if part != &scratch[..part.len()] {
                    return Err(std::io::Error::other("kernel recovery artifact differs"));
                }
            }
            self.remaining -= bytes.len() as u64;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut compare = Compare { file, remaining: length };
    borsh::to_writer(&mut compare, value).map_err(|e| format!("immutable kernel recovery artifact retained: {e}"))?;
    if compare.remaining != 0 || compare.file.metadata().map_err(|e| e.to_string())?.len() != length {
        return Err("kernel recovery artifact changed during comparison".into());
    }
    Ok(())
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
        if spec.source_lock_funding_outpoint.is_some() != spec.maximum_source_lock_fee_sompi.is_some()
            || spec.maximum_source_lock_fee_sompi == Some(0)
        {
            return Err("V6 source lock requires paired separate funding and positive explicit fee cap".into());
        }
        regular(&spec.work_path)?;
        regular(&spec.payer_key)?;
        for word in [&spec.checkpoint, &spec.paid_class, &spec.paid_job, &spec.salt] {
            hash(word)?;
        }
        Ok(spec)
    }
    pub fn prepare(self, ctx: &PalwNativeCustodyContextV3) -> Result<PreparedKernelPaidWorkV4, String> {
        // The paid-v4 private profile is PAV3 + material release recovery V2; the retired compact V4 fence is never armed.
        if ctx.config.params.palw_material_release_recovery_v2.is_none() || ctx.config.params.palw_claim_compact_v4.is_some() {
            return Err("explicit paid-v4/recovery configuration required (compact V4 is retired)".into());
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
        if let Some(raw) = &self.source_lock_funding_outpoint {
            let point = crate::palw_producer::parse_outpoint(raw)?;
            if ctx.config.params.palw_paid_bootstrap_source_v6.is_none()
                || funding.contains(&point)
                || point == producer.0
                || kaspa_consensus_core::config::drill::palw_t12_public_genesis_txid_v1(&point.transaction_id)
            {
                return Err("V6 lock funding needs explicit policy and a distinct private non-bond carrier input".into());
            }
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
        let shape = preflight_native_value_v1::<ProgramPaidBootstrapWorkV3>(&work_bytes[..], work_bytes.len() as u64)
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
            preflight_native_value_v1::<PalwConsensusObjectV2>(&bytes[..], bytes.len() as u64).map_err(|e| e.to_string())?;
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
        // A native read that races a tip/import/clock change (or an incomplete IBD/import) is retried on the next tick: these
        // reads bracket their own native point and refuse rather than mix two. Only the signed expiry turns waiting into a
        // terminal refusal; a draft, an upload, a queued carrier or a mempool entry is never treated as native acceptance.
        macro_rules! retry {
            ($what:expr, $e:expr) => {
                match $e {
                    Ok(v) => v,
                    Err(error) => {
                        kaspa_core::debug!("private kernel paid producer retries {}: {}", $what, error);
                        continue;
                    }
                }
            };
        }

        // Explicitly configuring V6 chooses the namespace for NEW unsigned
        // work. Wait for its real native activation instead of silently making
        // a legacy commit because a declared earliest DAA predates the fence.
        // A previously signed WAL always keeps its original namespace.
        let source_fence = self.config.params.palw_paid_bootstrap_source_v6;
        let source_policy = source_fence.map(|f| f.policy.id());
        if object.is_none() && source_fence.is_some_and(|f| !f.activation.is_active(self.spec.commit_daa)) {
            return Err("explicit V6 new work requires its declared commit DAA at/after activation; no legacy fallback".into());
        }
        if object.is_none() && source_policy.is_some() && self.spec.source_lock_funding_outpoint.is_none() {
            return Err("a new explicit V6 commit requires its independent source-lock input/fee authority".into());
        }
        let mut observed_enrollment = false;
        loop {
            if matches!(self.tick.tick(Duration::from_secs(2)).await, kaspa_core::task::tick::TickReason::Shutdown) {
                return Ok(());
            }
            let instance = self.manager.consensus();
            let api = instance.unguarded_session_blocking();
            let Some(route) = api.palw_kernel_route_v1() else {
                kaspa_core::debug!("kernel producer own native route unavailable; immutable WAL retained");
                continue;
            };
            let authority = match route.paid_claim_for_base_v2(&root) {
                Ok(authority) => authority,
                Err(error) => {
                    kaspa_core::debug!("kernel producer own enrollment unknown; immutable WAL retained: {error}");
                    continue;
                }
            };
            if let Some(authority) = authority {
                if authority.enrollment.class != class
                    || authority.enrollment.job != job
                    || authority.enrollment.producer
                        != kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_bond_id_v1(&self.producer)
                {
                    return Err("existing native paid authority differs from explicit work".into());
                }
                if !observed_enrollment {
                    kaspa_core::info!("private kernel paid enrollment accepted: {}", faster_hex::hex_string(&root));
                }
                observed_enrollment = true;
                match route.paid_bootstrap_binding_v6(&authority.enrollment.id()) {
                    Ok(Some(binding)) => {
                        if source_policy != Some(binding.policy) {
                            return Err("existing V6 binding differs from explicit producer policy; signed WAL retained".into());
                        }
                        self.drive_source_lock_v6(&route, &authority, &path).await?;
                    }
                    Ok(None) => {} // Old accepted/signed enrollment keeps its original namespace.
                    Err(error) => {
                        kaspa_core::debug!("V6 own binding unavailable; no source lock: {error}");
                    }
                }
                // Current application is reversible. Continue observing so a
                // rollback reuses the original signed object and exact spend.
                // This is neither Final nor permission to retire the WAL.
                continue;
            }
            observed_enrollment = false;

            // The producer signed this deadline; past it no carrier of this work can be admitted, so stop rather than spend.
            if api.get_virtual_daa_score() > self.spec.expires_daa {
                return Err("signed kernel work expired before native acceptance; durable outbox retained".into());
            }
            if object.is_none() {
                if source_fence.is_some_and(|f| !f.activation.is_active(api.get_virtual_daa_score())) {
                    continue;
                }
                if source_policy.is_some() {
                    use misaka_palw_kernel::claim_paid_v2::PaidClaimClassV2;
                    let raw = route.aux.iter().find_map(|((table, key), raw)| {
                        (*table == kaspa_consensus_core::palw_claim_challenge_v2::PAID_CLAIM_CLASSES_V2
                            && key.as_slice() == class.as_slice())
                        .then_some(raw)
                    });
                    let Some(raw) = raw else { continue };
                    let registered = match retain_native_value_decode_v1(raw, || borsh::from_slice::<PaidClaimClassV2>(raw)) {
                        Ok(registered) => registered,
                        Err(error) => {
                            kaspa_core::debug!("V6 producer class unavailable; no provider funding: {error}");
                            continue;
                        }
                    };
                    if !registered.supports_approved_bootstrap_source_v6() {
                        return Err(
                            "V6 private producer supports the explicit noninteractive direct consumer; no compact/staged fallback"
                                .into(),
                        );
                    }
                }
                if handoff.is_none() {
                    let candidate = retry!(
                        "own producer candidate",
                        api.palw_kernel_producer_candidate_v4(
                            checkpoint,
                            self.producer,
                            &work.commit,
                            &salt,
                            self.spec.commit_daa,
                            self.spec.expires_daa,
                        )
                    );
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
                    let grant = retry!("own native ingress grant", api.palw_kernel_material_ingress_v4(checkpoint, request));
                    // Every named provider must durably hold the material and sign; a partial or failed exchange is retried
                    // with a fresh grant (provider publication is idempotent per consent).
                    retry!("authenticated provider exchange", exchange.0.ingest_kernel_material_v4(request, &grant));
                    // Keep the sponsor's own copy of the signed work and its intrinsic context beside the outbox: a later
                    // bounded provider replacement (--private-probe-kernel-replace) replays exactly these, never a
                    // provider-supplied copy. Written once; an existing record is left untouched.
                    let request_path = path.join("request.borsh");
                    if !request_path.exists() {
                        publish_borsh(&request_path, request, self.spec.max_input_bytes)?;
                    }
                    let context_path = path.join("context.borsh");
                    if !context_path.exists() {
                        publish_borsh(&context_path, &*grant.context, self.spec.max_input_bytes)?;
                    }
                    ingress = Some(grant);
                }
                let grant = ingress.as_ref().ok_or("kernel ingress absent")?;
                // Own accepted prior funded enrollment. Requiring useful-work
                // finality here would create a first-work bootstrap cycle.
                // Reorgs are rechecked below and journal dependent claims.
                if !retry!(
                    "own accepted funded enrollment",
                    api.palw_kernel_material_admission_accepted_v4(request, &grant.enrollment)
                ) {
                    continue;
                }
                let PalwMaterialRecoveryActionV2::FundedEnroll { monetary, .. } = &*grant.enrollment else {
                    return Err("kernel funding draft absent".into());
                };
                // Producer-owned recovery artifacts. Publish the immutable
                // authenticated context and complete actual bundle before the
                // dependent claim is signed; replacement must not rewrite the
                // old intent/roster or trust an arbitrary peer projection.
                publish_artifact_or_match(&path.join("material-context-v4.borsh"), &*grant.context, self.spec.max_input_bytes)?;
                publish_artifact_or_match(&path.join("material-bundle-v4.borsh"), &request.bundle, self.spec.max_material_bytes)?;
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
                let inner = if let Some(policy) = source_policy {
                    misaka_palw_sdk::claim_paid_v2::paid_claim_commit_object_approved_bootstrap_v6(&route, policy, inner)
                        .map_err(|e| e.to_string())?
                } else {
                    inner
                };
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
            match api.palw_kernel_material_consent_accepted_v4(Hash64::from_bytes(root), consent) {
                Ok(true) => {}
                Ok(false) => continue,
                Err(error) => {
                    kaspa_core::debug!("kernel producer own consent unknown; immutable WAL retained: {error}");
                    continue;
                }
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
            if tx.inputs.len() != 1 || !self.funding.contains(&tx.inputs[0].previous_outpoint) {
                return Err("kernel saved carrier funding is outside current explicit authorization; WAL retained".into());
            }
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
            let Some(entry) = api.get_virtual_utxo_entry(tx.inputs[0].previous_outpoint) else {
                kaspa_core::debug!("kernel original funding unavailable; exact transaction retained");
                continue;
            };
            let fee = tx
                .outputs
                .iter()
                .try_fold(0u64, |sum, output| sum.checked_add(output.value))
                .and_then(|outputs| entry.amount.checked_sub(outputs));
            if entry.script_public_key != script
                || tx.outputs.len() != 1
                || tx.outputs[0].script_public_key != script
                || fee.is_none_or(|fee| fee > self.spec.maximum_fee_sompi)
            {
                return Err("kernel saved carrier exceeds current payer/change/fee authorization; WAL retained".into());
            }
            if api.palw_bond_payout_payload_v2(PalwBondKeyV2(tx.inputs[0].previous_outpoint)).is_some()
                || !crate::palw_panel::palw_fee_funding_usable_v1(
                    &tx.inputs[0].previous_outpoint,
                    &entry,
                    api.get_virtual_daa_score(),
                    self.config.params.coinbase_spend_maturity(),
                    None,
                    &std::collections::HashSet::new(),
                )
            {
                continue;
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
                Ok(Err(e)) => kaspa_core::warn!("private kernel claim durable relay refused: {e}"),
                Err(_) => kaspa_core::warn!("private kernel claim durable relay timed out"),
            }
        }
    }
}
impl PreparedKernelPaidWorkV4 {
    async fn drive_source_lock_v6(
        &self,
        observed: &kaspa_consensus_core::palw_kernel_route_v1::PalwKernelRouteStateV1,
        authority: &kaspa_consensus_core::palw_claim_challenge_v2::PalwPaidClaimAuthorityV2,
        path: &Path,
    ) -> Result<(), String> {
        let point = crate::palw_producer::parse_outpoint(
            self.spec
                .source_lock_funding_outpoint
                .as_deref()
                .ok_or("V6 source lock requires its original explicit funding authority")?,
        )?;
        drive_paid_source_lock_v6(
            PaidSourceLockContextV6 {
                manager: &self.manager,
                flow: &self.flow,
                config: &self.config,
                producer: self.producer,
                key: &self.key,
                payer: &self.payer,
                funding: point,
                maximum_fee_sompi: self.spec.maximum_source_lock_fee_sompi.ok_or("V6 lock fee authority absent")?,
                maximum_input_bytes: self.spec.max_input_bytes,
                maximum_workspace_bytes: self.spec.max_workspace_bytes,
            },
            observed,
            authority,
            path,
        )
        .await
    }
}
/// Called only while the shared original producer.lock is held. Both producer
/// and V7 continuation own the SAME immutable index17/carrier paths and budget.
pub(super) struct PaidSourceLockContextV6<'a> {
    pub manager: &'a kaspa_consensusmanager::ConsensusManager,
    pub flow: &'a kaspa_p2p_flows::flow_context::FlowContext,
    pub config: &'a kaspa_consensus_core::config::Config,
    pub producer: PalwBondKeyV2,
    pub key: &'a kaspa_pq_validator_core::ValidatorKey,
    pub payer: &'a libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair,
    pub funding: TransactionOutpoint,
    pub maximum_fee_sompi: u64,
    pub maximum_input_bytes: u64,
    pub maximum_workspace_bytes: u64,
}
fn source_lock_workspace_v6(maximum: u64, held: u64, wire: u64, decoded: u64) -> Result<(), String> {
    let demand =
        wire.checked_mul(4).and_then(|n| n.checked_add(decoded)).and_then(|n| n.checked_add(held)).and_then(|n| n.checked_add(65536));
    if demand.is_none_or(|n| n > maximum) {
        return Err("V6 source lock carrier exceeds explicit remaining workspace".into());
    }
    Ok(())
}
pub(super) async fn drive_paid_source_lock_v6(
    ctx: PaidSourceLockContextV6<'_>,
    observed: &kaspa_consensus_core::palw_kernel_route_v1::PalwKernelRouteStateV1,
    authority: &kaspa_consensus_core::palw_claim_challenge_v2::PalwPaidClaimAuthorityV2,
    path: &Path,
) -> Result<(), String> {
    use kaspa_consensus_core::palw_producer_v2::PalwObjectRehearsalV1;
    use misaka_palw_kernel::{claim_paid_v2::PaidClaimObjectV2, route::KernelRouteObjectV1};
    if ctx.maximum_fee_sompi == 0 || ctx.maximum_input_bytes == 0 || ctx.maximum_workspace_bytes == 0 {
        return Err("V6 lock requires finite original input/workspace/fee authority".into());
    }
    let enrollment = authority.enrollment.id();
    let point = ctx.funding;
    if point == ctx.producer.0 || kaspa_consensus_core::config::drill::palw_t12_public_genesis_txid_v1(&point.transaction_id) {
        return Err("V6 source-lock input names a producer bond or public genesis authority".into());
    }
    let fee_cap = ctx.maximum_fee_sompi;
    let instance = ctx.manager.consensus();
    let api = instance.unguarded_session_blocking();
    let own = match api.palw_source_bootstrap_epoch_observation_v1() {
        Ok(own) => own,
        Err(error) => {
            kaspa_core::debug!("V6 lock coherent own source authority unavailable: {error}");
            return Ok(());
        }
    };
    let Some(route) = own.state.kernel_route() else { return Ok(()) };
    let Some(policy) = ctx.config.params.palw_paid_bootstrap_source_v6.filter(|f| f.activation.is_active(own.virtual_daa)) else {
        return Ok(());
    };
    if route.header.policy.network_domain != observed.header.policy.network_domain {
        return Ok(());
    }
    if own.state.bond(&ctx.producer).is_none_or(|bond| {
        bond.pubkey.as_slice() != ctx.key.public_key()
            || !matches!(bond.status, kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Active)
    }) {
        return Ok(());
    }
    let binding = match route.paid_bootstrap_binding_v6(&enrollment) {
        Ok(Some(binding)) => binding,
        Ok(None) => return Ok(()),
        Err(error) => {
            kaspa_core::debug!("V6 lock own binding unreadable: {error}");
            return Ok(());
        }
    };
    if policy.policy.id() != binding.policy || binding.enrollment != enrollment {
        return Err("V6 lock binding changes explicit native enrollment/policy".into());
    }
    let current = match route.paid_claim_enrollment_checked_v6(&enrollment) {
        Ok(a) => a,
        Err(error) => {
            kaspa_core::debug!("V6 lock own enrollment unavailable: {error}");
            return Ok(());
        }
    };
    if current.enrollment != authority.enrollment {
        return Err("V6 lock own enrollment statement changed".into());
    }
    match route.paid_claim_effective_lock_checked_v6(&current) {
        Ok(Some(_)) => return Ok(()),
        Ok(None) => {}
        Err(error) => {
            kaspa_core::debug!("V6 effective lock unknown: {error}");
            return Ok(());
        }
    }
    let daa = own.virtual_daa;
    if !authority.enrollment.lifetime.receipt_open(daa) {
        return Ok(());
    }
    let object_path = path.join("source-lock-v6.borsh");
    let tx_path = path.join("source-lock-carrier-v6.borsh");
    if tx_path.exists() && !object_path.exists() {
        return Err("V6 source-lock carrier has no original signed object; no reconstruction".into());
    }
    let signing_quote = (std::mem::size_of::<PalwConsensusObjectV2>() as u64)
        .checked_add(kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN as u64)
        .and_then(|n| n.checked_add(4 * (std::mem::size_of::<KernelRouteObjectV1>() + 66) as u64))
        .ok_or("V6 lock signing request overflow")?;
    if signing_quote > ctx.maximum_workspace_bytes {
        return Ok(());
    }
    let _signing = reserve_native_requests_v1(signing_quote).map_err(|e| e.to_string())?;
    let inner = KernelRouteObjectV1::PaidClaimV2 { object: PaidClaimObjectV2::LockApprovedBootstrapV6 { enrollment } };
    let bytes = inner.encode();
    let domain = Hash64::from_bytes(route.header.policy.network_domain);
    let object = if object_path.exists() {
        let cap = ctx.maximum_input_bytes.min(ctx.maximum_workspace_bytes.saturating_sub(signing_quote) / 4);
        let (raw, _raw) = read_bounded(&object_path, cap)?;
        let shape = preflight_native_value_v1::<PalwConsensusObjectV2>(&raw[..], raw.len() as u64).map_err(|e| e.to_string())?;
        source_lock_workspace_v6(ctx.maximum_workspace_bytes, signing_quote, raw.len() as u64, shape.logical_requests_v1())?;
        retain_native_value_decode_v1(&raw, || borsh::from_slice::<PalwConsensusObjectV2>(&raw)).map_err(|e| e.to_string())?
    } else {
        let signed = misaka_palw_sdk::claim_paid_v2::sign_paid_claim_object_v2(ctx.key, domain, ctx.producer, &inner)?;
        publish_borsh(&object_path, &signed, ctx.maximum_input_bytes)?;
        NativeRetainedV1::retain_existing_v1(signed).map_err(|e| e.to_string())?
    };
    let PalwConsensusObjectV2::KernelRouteV1 { bytes: saved, signer, signature } = &*object else {
        return Err("V6 lock WAL object namespace differs".into());
    };
    let message = kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_route_message_v1(domain, &ctx.producer, &bytes);
    if *signer != ctx.producer
        || *saved != bytes
        || !kaspa_consensus_core::palw_attempt_material_v3::verify_ready_signature_v3(
            ctx.key.public_key(),
            message.as_byte_slice(),
            signature,
            kaspa_consensus_core::palw_kernel_route_v1::PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1,
        )
    {
        return Err("V6 lock immutable WAL substitutes enrollment/signature/purpose".into());
    }
    // The actual own fold re-derives approved source events and lineage.
    // A missing source or local material/history fault never spends a fee.
    if !matches!(api.palw_object_rehearsal_v1(&object), Some(PalwObjectRehearsalV1::Accepted)) {
        return Ok(());
    }
    let Some(entry) = api.get_virtual_utxo_entry(point) else { return Ok(()) };
    let script = kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk(
        &kaspa_hashes::blake2b_512_address_payload(ctx.payer.verification_key.as_ref()).as_bytes(),
    );
    if entry.script_public_key != script
        || api.palw_bond_payout_payload_v2(PalwBondKeyV2(point)).is_some()
        || !crate::palw_panel::palw_fee_funding_usable_v1(
            &point,
            &entry,
            daa,
            ctx.config.params.coinbase_spend_maturity(),
            None,
            &std::collections::HashSet::new(),
        )
    {
        return Ok(());
    }
    let (tx, _owner) = if tx_path.exists() {
        let cap = ctx.maximum_input_bytes.min(ctx.maximum_workspace_bytes.saturating_sub(signing_quote) / 4);
        let (raw, _raw) = read_bounded(&tx_path, cap)?;
        source_lock_workspace_v6(ctx.maximum_workspace_bytes, signing_quote, raw.len() as u64, 0)?;
        super::recovery_ack_carrier_v4::decode_carrier(&raw, &object)?
    } else {
        if ctx.flow.mining_manager().outpoint_is_spent_in_mempool(&point) {
            return Ok(());
        }
        let minimum = crate::palw_panel::palw_carrier_fees::palw_lifecycle_funding_minimum_v2(&ctx.config.params, &object, daa);
        if entry.amount < minimum {
            return Ok(());
        }
        source_lock_workspace_v6(
            ctx.maximum_workspace_bytes,
            signing_quote,
            borsh::object_length(&*object).map_err(|e| e.to_string())? as u64,
            0,
        )?;
        let owner =
            super::recovery_ack_carrier_v4::reserve_carrier_build_v4(&object, &entry, ctx.payer.verification_key.as_ref().len())?;
        let tx = crate::palw_panel::palw_carrier_fees::build_lifecycle_carrier_v2(
            ctx.config,
            ctx.payer,
            &object,
            point,
            &entry,
            &[],
            None,
            daa,
        )?;
        let fee = tx.outputs.iter().try_fold(0u64, |n, output| n.checked_add(output.value)).and_then(|n| entry.amount.checked_sub(n));
        if fee.is_none_or(|fee| fee > fee_cap) {
            return Err("V6 lock carrier exceeds its explicit fee ceiling".into());
        }
        publish_borsh(&tx_path, &tx, ctx.maximum_input_bytes)?;
        (tx, owner)
    };
    let fee = tx.outputs.iter().try_fold(0u64, |n, output| n.checked_add(output.value)).and_then(|n| entry.amount.checked_sub(n));
    if tx.inputs.len() != 1
        || tx.inputs[0].previous_outpoint != point
        || tx.outputs.len() != 1
        || tx.outputs[0].script_public_key != script
        || fee.is_none_or(|fee| fee > fee_cap)
    {
        return Err("V6 lock carrier WAL substitutes input/change/fee authority".into());
    }
    if ctx
        .flow
        .mining_manager()
        .clone()
        .has_transaction(tx.id(), kaspa_mining::model::tx_query::TransactionQuery::TransactionsOnly)
        .await
    {
        return Ok(());
    }
    // Do not broadcast across a changed own import/branch observation after
    // the asynchronous mempool query. A retry keeps the same index17 and input.
    let fresh = match api.palw_source_bootstrap_epoch_observation_v1() {
        Ok(fresh) => fresh,
        Err(error) => {
            kaspa_core::debug!("V6 lock pre-relay own authority unavailable: {error}");
            return Ok(());
        }
    };
    if fresh.checkpoint != own.checkpoint || fresh.import_epoch != own.import_epoch || fresh.virtual_daa != own.virtual_daa {
        return Ok(());
    }
    if !matches!(api.palw_object_rehearsal_v1(&object), Some(PalwObjectRehearsalV1::Accepted)) {
        return Ok(());
    }
    let _relay = super::recovery_ack_carrier_v4::reserve_carrier_build_v4(&object, &entry, ctx.payer.verification_key.as_ref().len())?;
    match tokio::time::timeout(
        Duration::from_secs(10),
        ctx.flow.submit_rpc_transaction(&instance.unguarded_session(), tx, kaspa_mining::mempool::tx::Orphan::Forbidden),
    )
    .await
    {
        Ok(Ok(())) => {}
        Ok(Err(error)) => kaspa_core::debug!("V6 lock exact carrier awaiting native acceptance: {error}"),
        Err(_) => kaspa_core::debug!("V6 lock exact carrier submission timed out"),
    }
    Ok(())
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
    use misaka_palw_kernel::{claim_paid_v2::PaidClaimObjectV2, route::KernelRouteObjectV1};
    let (enrollment, commit, saved_salt) = match &*inner {
        KernelRouteObjectV1::PaidClaimV2 {
            object:
                PaidClaimObjectV2::Commit { enrollment, commit, salt }
                | PaidClaimObjectV2::CommitWithApprovedBootstrapV6 { enrollment, commit, salt, .. },
        } => (enrollment, commit, salt),
        _ => return Err("saved kernel object is not its original paid commit namespace".into()),
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

#[cfg(test)]
mod recovery_artifact_controls {
    use super::*;

    #[test]
    fn crash_before_claim_wal_reuses_only_exact_artifact_and_preserves_mismatches() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("material-bundle-v4.borsh");
        let value = vec![37u8; 9000]; // Exercises multiple fixed comparison chunks.
        let cap = 10000;
        publish_artifact_or_match(&path, &value, cap).unwrap();
        let original = fs::read(&path).unwrap();
        // Reopen after the artifact publication but before any signed claim.
        publish_artifact_or_match(&path, &value, cap).unwrap();
        let mut changed = value.clone();
        changed[4097] ^= 1;
        assert!(publish_artifact_or_match(&path, &changed, cap).is_err());
        assert!(publish_artifact_or_match(&path, &value, 8000).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
        let mut trailing = original.clone();
        trailing.push(0);
        fs::write(&path, &trailing).unwrap();
        assert!(publish_artifact_or_match(&path, &value, cap).is_err());
        assert_eq!(fs::read(&path).unwrap(), trailing, "mismatch is retained, never silently overwritten");
    }
}

#[cfg(test)]
mod source_lock_controls_v6 {
    use super::*;
    #[test]
    fn restored_lock_carrier_overlap_never_exceeds_the_remaining_authorization() {
        // Allocation arithmetic only; this does not submit or accept a Lock.
        let required = 4 * 100 + 80 + 20 + 65536;
        assert!(source_lock_workspace_v6(required, 80, 100, 20).is_ok());
        assert!(source_lock_workspace_v6(required - 1, 80, 100, 20).is_err());
        assert!(source_lock_workspace_v6(u64::MAX, 80, u64::MAX, 20).is_err());
    }
}
