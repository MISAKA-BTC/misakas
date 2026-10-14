//! **RFC-0008 PAV3 loopback provider for the private paid-v3 profile** (lead default B, 2026-10-15; unverified source,
//! never compiled or run here). Feature-gated with the rest of `private_integration`: ordinary kaspad Args cannot select it.
//!
//! A provider service owns, per configured fixture provider bond, a [`DurableMonetaryMaterialProviderV1`] store under the
//! node's private data dir, and signs a producer's Ready only after it
//!
//! 1. obtained the material authority from ITS OWN node's Consensus API
//!    (`palw_native_material_provider_authority_v3`, which re-runs the canonical session selection at the checkpoint),
//! 2. re-derived the job prompt from the anchor and rebuilt the complete typed bundle from the capture with its own backend
//!    (`palw_native_custody_v3::palw_native_bundle_v3`, the one function the producer uses too), and found the producer's
//!    manifest identical,
//! 3. checked that the chain's registered key for its roster bond is the key it holds, and that the executor is not one of
//!    its own operators,
//! 4. durably published the bundle (`misaka_palw_remote::native_ready_v3::provider_sign_native_ready_v3`).
//!
//! Keys are the private fixture's seat seeds (the files `export_private_complete_bootstrap_v1` writes, loaded with
//! `load_validator_seed` like every private seat key) named with their bond outpoints; each must be the salt's own card for
//! that bond, at least two distinct card operators are required, and none may be the local producer's key, bond or operator.
//! Transport: in-process (this node's own producer) or one framed request per connection over a loopback TCP socket.
use crate::palw_native_custody_v3::{
    NativeReadyExchangeV3, PALW_NATIVE_READY_EXCHANGE_VERSION_V3, PalwNativeCustodyContextV3, PalwNativeCustodyHookV3,
    PalwNativeExchangeHandleV3, PalwNativeReadyRequestV3, PalwNativeReadyResponseV3, palw_native_bundle_v3, palw_native_prompt_v3,
};
use kaspa_consensus_core::{
    palw_material_capacity_v1::PalwMaterialNativeAuthorityV1,
    palw_prompt_ids_v1::PalwPromptIdsFormV1,
    palw_state_v2::{PalwBondKeyV2, palw_operator_id_v2},
};
use kaspa_hashes::Hash64;
use misaka_palw_remote::monetary_material::DurableMonetaryMaterialProviderV1;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

/// One framed request or response; a private fixture capture is small, and an unbounded length is refused.
pub const PRIVATE_NATIVE_EXCHANGE_MAX_FRAME_V3: u64 = 1 << 30;
/// How long one exchange may take end to end (two bundle rebuilds and fsyncs on a private fixture class).
pub const PRIVATE_NATIVE_EXCHANGE_TIMEOUT_V3: Duration = Duration::from_secs(600);

/// One configured provider bond and the fixture seed that signs for it.
pub struct PrivateNativeProviderKeyV3 {
    pub bond: PalwBondKeyV2,
    pub seed_path: PathBuf,
    key: kaspa_pq_validator_core::ValidatorKey,
}

/// **The private native-custody wiring**, parsed from the private node's `--private-probe-native-*` options only.
pub struct PrivateNativeCustodySpecV3 {
    /// Provider bonds this node serves (empty: none).
    pub providers: Vec<PrivateNativeProviderKeyV3>,
    /// Serve them to other private nodes on this loopback socket. Without it they serve this node's own producer in-process.
    pub listen: Option<SocketAddr>,
    /// This node's producer asks the provider service at this loopback socket.
    pub exchange: Option<SocketAddr>,
}

fn loopback(name: &str, value: &str) -> Result<SocketAddr, String> {
    let addr: SocketAddr = value.parse().map_err(|e| format!("{name}={value}: {e}"))?;
    if !addr.ip().is_loopback() || addr.port() == 0 {
        return Err(format!("{name} must be an explicit loopback address with a port"));
    }
    Ok(addr)
}

impl PrivateNativeCustodySpecV3 {
    /// `keys`: `SEED_FILE@TXID:INDEX[,SEED_FILE@TXID:INDEX...]`. `None` when no option was given.
    pub fn parse(keys: Option<&str>, listen: Option<&str>, exchange: Option<&str>) -> Result<Option<Self>, String> {
        if keys.is_none() && listen.is_none() && exchange.is_none() {
            return Ok(None);
        }
        let mut providers = Vec::new();
        if let Some(keys) = keys {
            for entry in keys.split(',') {
                let (file, bond) =
                    entry.rsplit_once('@').ok_or_else(|| format!("provider key '{entry}' is not SEED_FILE@TXID:INDEX"))?;
                let path = PathBuf::from(file);
                if !path.is_absolute() {
                    return Err(format!("provider seed file {file} must be an absolute fixture path"));
                }
                let seed = kaspa_pq_validator_core::load_validator_seed(file)?;
                let bond = PalwBondKeyV2(crate::palw_producer::parse_outpoint(bond)?);
                providers.push(PrivateNativeProviderKeyV3 {
                    bond,
                    seed_path: path,
                    key: kaspa_pq_validator_core::ValidatorKey::from_seed(seed),
                });
            }
            let bonds: BTreeSet<_> = providers.iter().map(|p| p.bond).collect();
            let keys: BTreeSet<_> = providers.iter().map(|p| p.key.public_key().to_vec()).collect();
            if bonds.len() != providers.len() || keys.len() != providers.len() {
                return Err("private native providers need distinct bonds and distinct keys".into());
            }
            if providers.len() < 2 || providers.len() > kaspa_consensus_core::palw_attempt_material_v3::PALW_MATERIAL_MAX_PROVIDERS_V3
            {
                return Err("a private native provider service holds 2..=8 roster bonds".into());
            }
        }
        let listen = listen.map(|v| loopback("--private-probe-native-provider-listen", v)).transpose()?;
        let exchange = exchange.map(|v| loopback("--private-probe-native-exchange", v)).transpose()?;
        if listen.is_some() && providers.is_empty() {
            return Err("--private-probe-native-provider-listen needs --private-probe-native-provider-keys".into());
        }
        if exchange.is_some() && !providers.is_empty() && listen.is_none() {
            return Err("in-process providers and a socket exchange are exclusive for one producer".into());
        }
        if exchange.is_some() && exchange == listen {
            return Err("a node does not dial its own provider socket; omit the listener to serve it in-process".into());
        }
        Ok(Some(Self { providers, listen, exchange }))
    }

    /// **The fixture check**: each provider seed is the salt's card key for that exact bond; at least two distinct card
    /// operators; and, on a node that produces, none is its producer's bond, key or operator (an observer that names a seat's
    /// producer key without `--palw-produce` may host other seats). Runs before any service or DB opens. A node reopened
    /// without `--palw-produce` keeps an unused exchange: nothing calls it.
    pub fn check_fixture(&self, config: &kaspa_consensus_core::config::Config, args: &crate::args::Args) -> Result<(), String> {
        let salt = config.palw_drill_genesis_salt.as_ref().ok_or("private native providers need the salted fixture")?;
        let cards: BTreeMap<PalwBondKeyV2, kaspa_consensus_core::config::drill::PalwDrillCardV1> =
            kaspa_consensus_core::config::drill::palw_t12_drill_cards_v1(salt)
                .into_iter()
                .map(|card| {
                    (
                        PalwBondKeyV2(kaspa_consensus_core::config::drill::palw_t12_drill_premine_outpoint_v1(
                            salt,
                            card.premine_index,
                        )),
                        card,
                    )
                })
                .collect();
        let mut operators = BTreeSet::new();
        for provider in &self.providers {
            let card = cards.get(&provider.bond).ok_or("a private native provider bond is not a fixture seat")?;
            if card.bond.pubkey.as_slice() != provider.key.public_key() {
                return Err(format!("provider seed {} is not the fixture key of its bond", provider.seed_path.display()));
            }
            operators.insert(palw_operator_id_v2(&card.operator.pubkey));
        }
        if !self.providers.is_empty() && operators.len() < 2 {
            return Err("private native providers need at least two distinct operators".into());
        }
        if let Some(bond) = args.palw_producer_bond.as_ref().filter(|_| args.palw_produce) {
            let bond = PalwBondKeyV2(crate::palw_producer::parse_outpoint(bond)?);
            if self.providers.iter().any(|p| p.bond == bond) {
                return Err("a private native provider is this node's own producer bond".into());
            }
            if let Some(card) = cards.get(&bond)
                && operators.contains(&palw_operator_id_v2(&card.operator.pubkey))
            {
                return Err("a private native provider shares the executor's operator; self-custody is refused".into());
            }
        }
        if let Some(path) = args.palw_producer_key.as_ref().filter(|_| args.palw_produce) {
            let producer = kaspa_pq_validator_core::ValidatorKey::from_seed(kaspa_pq_validator_core::load_validator_seed(path)?);
            if self.providers.iter().any(|p| p.key.public_key() == producer.public_key()) {
                return Err("a private native provider key is this node's own producer key".into());
            }
        }
        Ok(())
    }

    /// The daemon hook: open the provider service (and its loopback listener), and return the exchange for this node's producer.
    pub fn into_hook(self) -> PalwNativeCustodyHookV3 {
        Box::new(move |ctx: PalwNativeCustodyContextV3| -> Result<Option<PalwNativeExchangeHandleV3>, String> {
            let Self { providers, listen, exchange } = self;
            let service = if providers.is_empty() { None } else { Some(Arc::new(LoopbackNativeProviderV3::open(&ctx, providers)?)) };
            if let (Some(service), Some(listen)) = (&service, listen) {
                let listener = TcpListener::bind(listen).map_err(|e| format!("native provider listener {listen}: {e}"))?;
                let service = Arc::clone(service);
                std::thread::Builder::new()
                    .name("palw-native-provider-v3".into())
                    .spawn(move || serve_loopback_v3(listener, service))
                    .map_err(|e| e.to_string())?;
                kaspa_core::warn!("RFC12 PRIVATE: PAV3 loopback native provider service listening on {listen} (unapproved mechanics)");
            }
            Ok(match (exchange, service, listen) {
                (Some(addr), _, _) => Some(PalwNativeExchangeHandleV3(Arc::new(LoopbackSocketExchangeV3 { addr }))),
                (None, Some(service), None) => Some(PalwNativeExchangeHandleV3(service)),
                _ => None,
            })
        })
    }
}

struct ServedProviderV3 {
    bond: PalwBondKeyV2,
    operator: Hash64,
    key: kaspa_pq_validator_core::ValidatorKey,
    store: Mutex<DurableMonetaryMaterialProviderV1>,
    authorities: Mutex<AuthorityIndexV3>,
}

/// **The in-node provider service.** Holds no consensus state of its own: every authority is a fresh Consensus API read.
pub struct LoopbackNativeProviderV3 {
    consensus_manager: Arc<kaspa_consensusmanager::ConsensusManager>,
    config: Arc<kaspa_consensus_core::config::Config>,
    court: kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2,
    holdings: Vec<misaka_palw_sdk::PalwLoadedArtifactV1>,
    chain_classes: bool,
    providers: Vec<ServedProviderV3>,
}

impl LoopbackNativeProviderV3 {
    fn open(ctx: &PalwNativeCustodyContextV3, keys: Vec<PrivateNativeProviderKeyV3>) -> Result<Self, String> {
        let params = &ctx.config.params;
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
            return Err("a native provider needs a ConsensusV2 network".into());
        };
        let policy = &params.palw_monetary_material_v1.as_ref().ok_or("a native provider needs the monetary material fence")?.policy;
        let salt = ctx.config.palw_drill_genesis_salt.as_ref().ok_or("a native provider needs the salted private fixture")?;
        let operators: BTreeMap<PalwBondKeyV2, Hash64> = kaspa_consensus_core::config::drill::palw_t12_drill_cards_v1(salt)
            .into_iter()
            .map(|card| {
                (
                    PalwBondKeyV2(kaspa_consensus_core::config::drill::palw_t12_drill_premine_outpoint_v1(salt, card.premine_index)),
                    palw_operator_id_v2(&card.operator.pubkey),
                )
            })
            .collect();
        let dir = ctx.data_dir.join("palw-native-provider-v3");
        let mut providers = Vec::new();
        for key in keys {
            let file = dir.join(format!("{}-{}.store", key.bond.0.transaction_id, key.bond.0.index));
            let authorities = AuthorityIndexV3::open(&file.with_extension("authorities"))?;
            let store = DurableMonetaryMaterialProviderV1::open(
                &file,
                policy.max_bytes_per_contract,
                policy.max_bytes_per_provider,
                None,
                |claim| authorities.get(&claim),
            )?;
            let operator = *operators.get(&key.bond).ok_or("a native provider bond is not a fixture seat")?;
            providers.push(ServedProviderV3 {
                bond: key.bond,
                operator,
                key: key.key,
                store: Mutex::new(store),
                authorities: Mutex::new(authorities),
            });
        }
        let form = params.palw_prompt_ids_form_v1();
        let net = params.net.to_string().into_bytes();
        let sdk = misaka_palw_sdk::PalwClassSdk::builtin_v1(bundle.court, form, net);
        let holdings = crate::palw_backends::load_class_holdings_v1(
            "palw-native-provider",
            &sdk,
            &ctx.class_artifacts,
            ctx.class_cache_bytes,
            misaka_palw_sdk::PalwWeightResidencyV1::PageCache,
        );
        Ok(Self {
            consensus_manager: Arc::clone(&ctx.consensus_manager),
            config: Arc::clone(&ctx.config),
            court: bundle.court,
            holdings,
            chain_classes: ctx.chain_classes,
            providers,
        })
    }

    /// One request, every roster bond this service holds: each signature only after its own durable publication.
    pub fn sign_request(&self, request: &PalwNativeReadyRequestV3) -> Result<Vec<(PalwBondKeyV2, Vec<u8>)>, String> {
        if request.version != PALW_NATIVE_READY_EXCHANGE_VERSION_V3 {
            return Err("unknown native exchange version".into());
        }
        let ready = &request.envelope.ready;
        let attempt = &request.envelope.base.attempt;
        let ours: Vec<&ServedProviderV3> = self.providers.iter().filter(|p| ready.providers.contains(&p.bond)).collect();
        if ours.is_empty() {
            return Err("none of this service's provider bonds is on the session roster".into());
        }
        if ours.iter().any(|p| p.operator == attempt.operator_id) {
            return Err("the executor is one of this service's provider operators; self-custody is refused".into());
        }
        let session = self.consensus_manager.consensus().unguarded_session();
        let authority = session.palw_native_material_provider_authority_v3(request.checkpoint, &request.header, &request.envelope)?;
        let params = &self.config.params;
        let registry = crate::palw_backends::PalwBackendRegistry::for_node_v1(
            params,
            self.court,
            params.palw_prompt_ids_form_v1(),
            self.holdings.clone(),
            params.net.to_string().into_bytes(),
            self.chain_classes,
        );
        let backend = registry.resolve_or_chain(attempt.class_id, attempt.artifact_root, |id| {
            if self.chain_classes { session.palw_registered_class_carriage_v1(id) } else { None }
        })?;
        drop(session);
        // The authority answer validated the attempt's network domain, position and signature: the anchor is the header's.
        let anchor = kaspa_consensus_core::palw_attempt_v2::execution_anchor_v3(
            attempt.network_domain,
            kaspa_consensus_core::hashing::header::pre_pow_hash_64(&request.header),
            attempt.class_id,
            &attempt.executor_bond,
            request.header.nonce,
        );
        let prompt = palw_native_prompt_v3(backend.as_ref(), anchor)?;
        if prompt != request.prompt {
            return Err("the request's prompt is not the one its anchor implies".into());
        }
        let prefill_draw = params.palw_prefill_draw_active_at(request.header.daa_score);
        let (bundle, manifest) = palw_native_bundle_v3(
            backend.as_ref(),
            &authority.row,
            attempt,
            anchor,
            &request.capture,
            &prompt,
            prefill_draw,
            authority.material,
        )?;
        if manifest != ready.binding.manifest {
            return Err("the producer's manifest differs from the bundle this provider rebuilt".into());
        }
        let mut signatures = Vec::new();
        for provider in ours {
            let (_, registered) = authority
                .provider_keys
                .iter()
                .find(|(bond, _)| *bond == provider.bond)
                .ok_or("the chain names no key for this roster bond")?;
            if registered.as_slice() != provider.key.public_key() {
                return Err("the chain's registered key for this roster bond is not the key this service holds".into());
            }
            // The reopen index first: a crash after publication must still find the authority to re-verify the bundle.
            provider.authorities.lock().map_err(|_| "authority index poisoned")?.record(bundle.claim, authority.material)?;
            let mut store = provider.store.lock().map_err(|_| "provider store poisoned")?;
            let signature = misaka_palw_remote::native_ready_v3::provider_sign_native_ready_v3(
                &mut store,
                provider.bond,
                &authority.row,
                ready,
                &bundle,
                authority.material,
                |message, context| Ok(provider.key.sign_with_context(message, context).to_vec()),
            )?;
            signatures.push((provider.bond, signature));
        }
        Ok(signatures)
    }
}

impl NativeReadyExchangeV3 for LoopbackNativeProviderV3 {
    fn describe(&self) -> String {
        format!("in-process loopback provider ({} bond(s))", self.providers.len())
    }
    fn request_signatures(&self, request: &PalwNativeReadyRequestV3) -> Result<BTreeMap<PalwBondKeyV2, Vec<u8>>, String> {
        Ok(self.sign_request(request)?.into_iter().collect())
    }
}

/// The client half: one connection per request to a loopback provider service in another private node.
pub struct LoopbackSocketExchangeV3 {
    pub addr: SocketAddr,
}

impl NativeReadyExchangeV3 for LoopbackSocketExchangeV3 {
    fn describe(&self) -> String {
        format!("loopback provider socket {}", self.addr)
    }
    fn request_signatures(&self, request: &PalwNativeReadyRequestV3) -> Result<BTreeMap<PalwBondKeyV2, Vec<u8>>, String> {
        let mut stream = TcpStream::connect_timeout(&self.addr, Duration::from_secs(10)).map_err(|e| e.to_string())?;
        stream.set_read_timeout(Some(PRIVATE_NATIVE_EXCHANGE_TIMEOUT_V3)).map_err(|e| e.to_string())?;
        stream.set_write_timeout(Some(PRIVATE_NATIVE_EXCHANGE_TIMEOUT_V3)).map_err(|e| e.to_string())?;
        write_frame_v3(&mut stream, &borsh::to_vec(request).map_err(|e| e.to_string())?)?;
        let response: PalwNativeReadyResponseV3 = borsh::from_slice(&read_frame_v3(&mut stream)?).map_err(|e| e.to_string())?;
        match response {
            PalwNativeReadyResponseV3::Signed { version: PALW_NATIVE_READY_EXCHANGE_VERSION_V3, signatures } => {
                let count = signatures.len();
                let map: BTreeMap<_, _> = signatures.into_iter().collect();
                if map.len() != count {
                    return Err("the provider service answered one bond twice".into());
                }
                Ok(map)
            }
            PalwNativeReadyResponseV3::Refused { reason, .. } => Err(format!("provider refused: {reason}")),
            PalwNativeReadyResponseV3::Signed { .. } => Err("unknown native exchange response version".into()),
        }
    }
}

/// The service half: loopback peers only, one request per connection, served in order (the stores are serialized anyway).
fn serve_loopback_v3(listener: TcpListener, service: Arc<LoopbackNativeProviderV3>) {
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        if !stream.peer_addr().is_ok_and(|peer| peer.ip().is_loopback()) {
            continue;
        }
        let _ = stream.set_read_timeout(Some(PRIVATE_NATIVE_EXCHANGE_TIMEOUT_V3));
        let _ = stream.set_write_timeout(Some(PRIVATE_NATIVE_EXCHANGE_TIMEOUT_V3));
        let response = match read_frame_v3(&mut stream)
            .and_then(|bytes| borsh::from_slice::<PalwNativeReadyRequestV3>(&bytes).map_err(|e| e.to_string()))
            .and_then(|request| service.sign_request(&request))
        {
            Ok(signatures) => PalwNativeReadyResponseV3::Signed { version: PALW_NATIVE_READY_EXCHANGE_VERSION_V3, signatures },
            Err(reason) => {
                kaspa_core::warn!("RFC12 PRIVATE: PAV3 native provider refused a request: {reason}");
                PalwNativeReadyResponseV3::Refused { version: PALW_NATIVE_READY_EXCHANGE_VERSION_V3, reason }
            }
        };
        if let Ok(bytes) = borsh::to_vec(&response) {
            let _ = write_frame_v3(&mut stream, &bytes);
        }
    }
}

fn write_frame_v3(stream: &mut impl Write, bytes: &[u8]) -> Result<(), String> {
    if bytes.len() as u64 > PRIVATE_NATIVE_EXCHANGE_MAX_FRAME_V3 {
        return Err("native exchange frame exceeds its cap".into());
    }
    stream
        .write_all(&(bytes.len() as u64).to_le_bytes())
        .and_then(|_| stream.write_all(bytes))
        .and_then(|_| stream.flush())
        .map_err(|e| e.to_string())
}

fn read_frame_v3(stream: &mut impl Read) -> Result<Vec<u8>, String> {
    let mut length = [0u8; 8];
    stream.read_exact(&mut length).map_err(|e| e.to_string())?;
    let length = u64::from_le_bytes(length);
    if length > PRIVATE_NATIVE_EXCHANGE_MAX_FRAME_V3 {
        return Err("native exchange frame exceeds its cap".into());
    }
    let mut bytes = vec![0u8; length as usize];
    stream.read_exact(&mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes)
}

/// The authority each held bundle was admitted under, kept beside the store so a reopen re-verifies every bundle
/// (`DurableMonetaryMaterialProviderV1::open`'s recovery asks for it per claim). Written whole, fsynced and renamed, BEFORE the
/// bundle is published: an entry without a bundle is harmless, a bundle without its entry would refuse the reopen.
struct AuthorityIndexV3 {
    file: PathBuf,
    rows: BTreeMap<Hash64, PalwMaterialNativeAuthorityV1>,
}

type AuthorityRowV3 = (Hash64, Hash64, Hash64, Hash64, u64, u8);

impl AuthorityIndexV3 {
    fn open(file: &Path) -> Result<Self, String> {
        let rows = match std::fs::read(file) {
            Ok(bytes) => borsh::from_slice::<Vec<AuthorityRowV3>>(&bytes)
                .map_err(|e| format!("native provider authority index: {e}"))?
                .into_iter()
                .map(|(claim, execution_root, class_id, artifact_root, ladder, form)| {
                    let form = match form {
                        0 => PalwPromptIdsFormV1::Flat,
                        1 => PalwPromptIdsFormV1::MerkleV1,
                        _ => return Err("native provider authority index names an unknown prompt form".to_string()),
                    };
                    Ok((claim, PalwMaterialNativeAuthorityV1 { execution_root, class_id, artifact_root, ladder, form }))
                })
                .collect::<Result<_, _>>()?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(error) => return Err(error.to_string()),
        };
        Ok(Self { file: file.to_path_buf(), rows })
    }
    fn get(&self, claim: &Hash64) -> Option<PalwMaterialNativeAuthorityV1> {
        self.rows.get(claim).copied()
    }
    fn record(&mut self, claim: Hash64, authority: PalwMaterialNativeAuthorityV1) -> Result<(), String> {
        match self.rows.get(&claim) {
            Some(known) if *known == authority => return Ok(()),
            Some(_) => return Err("a claim's native authority changed between two Ready requests".into()),
            None => {}
        }
        let mut rows = self.rows.clone();
        rows.insert(claim, authority);
        let encoded: Vec<AuthorityRowV3> = rows
            .iter()
            .map(|(claim, a)| {
                let form = match a.form {
                    PalwPromptIdsFormV1::Flat => 0,
                    PalwPromptIdsFormV1::MerkleV1 => 1,
                };
                (*claim, a.execution_root, a.class_id, a.artifact_root, a.ladder, form)
            })
            .collect();
        let parent = self.file.parent().ok_or("authority index has no parent directory")?;
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let temporary = self.file.with_extension("authorities.partial");
        let result = (|| {
            let mut out = std::fs::File::create(&temporary)?;
            out.write_all(&borsh::to_vec(&encoded)?)?;
            out.sync_all()?;
            std::fs::rename(&temporary, &self.file)?;
            std::fs::File::open(parent)?.sync_all()
        })();
        if let Err(error) = result {
            let _ = std::fs::remove_file(&temporary);
            return Err(error.to_string());
        }
        self.rows = rows;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_private_options_are_loopback_only_and_need_two_distinct_bonds() {
        assert!(PrivateNativeCustodySpecV3::parse(None, None, None).unwrap().is_none());
        assert!(PrivateNativeCustodySpecV3::parse(None, None, Some("10.0.0.1:7000")).is_err(), "not loopback");
        assert!(PrivateNativeCustodySpecV3::parse(None, None, Some("127.0.0.1:0")).is_err(), "no port");
        assert!(PrivateNativeCustodySpecV3::parse(None, Some("127.0.0.1:7000"), None).is_err(), "a listener with no keys");
        let spec = PrivateNativeCustodySpecV3::parse(None, None, Some("127.0.0.1:7000")).unwrap().unwrap();
        assert!(spec.providers.is_empty() && spec.listen.is_none());
        let dir = tempfile::tempdir().unwrap();
        let seed = |n: u8| {
            let path = dir.path().join(format!("bond-{n}.seed"));
            kaspa_pq_validator_core::write_validator_seed(path.to_str().unwrap(), &[n; 32]).unwrap();
            path.to_str().unwrap().to_owned()
        };
        let (a, b) = (seed(1), seed(2));
        let txid = Hash64::from_u64_word(9);
        let keys = format!("{a}@{txid}:1,{b}@{txid}:2");
        let spec = PrivateNativeCustodySpecV3::parse(Some(&keys), Some("127.0.0.1:7001"), None).unwrap().unwrap();
        assert_eq!(spec.providers.len(), 2);
        assert!(PrivateNativeCustodySpecV3::parse(Some(&format!("{a}@{txid}:1")), None, None).is_err(), "one bond is no roster");
        assert!(PrivateNativeCustodySpecV3::parse(Some(&format!("{a}@{txid}:1,{a}@{txid}:2")), None, None).is_err(), "one key twice");
        assert!(
            PrivateNativeCustodySpecV3::parse(Some(&keys), None, Some("127.0.0.1:7002")).is_err(),
            "in-process providers and a socket exchange are exclusive"
        );
        assert!(PrivateNativeCustodySpecV3::parse(Some(&keys), Some("127.0.0.1:7003"), Some("127.0.0.1:7003")).is_err());
    }

    #[test]
    fn a_frame_round_trips_and_an_oversized_length_is_refused() {
        let mut wire = Vec::new();
        write_frame_v3(&mut wire, b"ready").unwrap();
        assert_eq!(read_frame_v3(&mut wire.as_slice()).unwrap(), b"ready");
        let mut huge = (PRIVATE_NATIVE_EXCHANGE_MAX_FRAME_V3 + 1).to_le_bytes().to_vec();
        huge.extend([0u8; 8]);
        assert!(read_frame_v3(&mut huge.as_slice()).is_err());
    }

    #[test]
    fn the_authority_index_survives_a_reopen_and_refuses_a_changed_authority() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("p.authorities");
        let a = PalwMaterialNativeAuthorityV1 {
            execution_root: Hash64::from_u64_word(1),
            class_id: Hash64::from_u64_word(2),
            artifact_root: Hash64::from_u64_word(3),
            ladder: 1 << 22,
            form: PalwPromptIdsFormV1::MerkleV1,
        };
        let claim = Hash64::from_u64_word(4);
        AuthorityIndexV3::open(&file).unwrap().record(claim, a).unwrap();
        let mut reopened = AuthorityIndexV3::open(&file).unwrap();
        assert_eq!(reopened.get(&claim), Some(a));
        reopened.record(claim, a).unwrap();
        assert!(reopened.record(claim, PalwMaterialNativeAuthorityV1 { ladder: 1, ..a }).is_err());
    }
}
