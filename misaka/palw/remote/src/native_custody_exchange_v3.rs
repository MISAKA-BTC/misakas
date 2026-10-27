//! **RFC-0009 × RFC-0008 PAV3: the producer's provider exchange for the shipping remote miner** (G14 lane rfc09-13-19,
//! 2026-10-22; re-cut onto the provider service's wire by audit fix-cd C-03).
//!
//! **One wire.** The provider side that exists is the provider service (`kaspad::private_integration::native_provider`): it reads
//! one length-framed [`ProviderTransportRequestV1`] per TCP connection and derives the material authority from ITS OWN node at the
//! request's checkpoint, from the candidate header and the signed base attempt — it never trusts the producer's account of the
//! session. This module now speaks exactly that wire: the request/response bodies below are the provider service's (kaspad
//! re-exports them, so the two cannot drift), framed by [`write_native_frame_v3`] / [`read_native_frame_v3`]. The earlier
//! HTTP `POST /palw/native-ready/v3` `{Manifest, Sign}` exchange had no server anywhere and is gone.
//!
//! A remote miner links no model backend, so it cannot build the typed bundle (and its manifest) or derive the prompt from the
//! anchor itself. The one addition to the provider's wire is therefore a read-only purpose,
//! [`ProviderTransportPurposeV1::NativeManifestV3`]: the same request, carrying the DRAW envelope (the placeholder manifest the
//! ticket is priced with, `palw_native_draw_manifest_v3`), answered with the manifest and prompt the provider rebuilt. The flow,
//! each step refusing rather than degrading:
//!
//! 1. **Session open** ([`NativeCustodyV3::session`]): `select_native_session_partial_view_v3` over the AUTHENTICATED rows — the
//!    canonical row `verify_prior` re-selects, or a hold (audit C-02).
//! 2. **Draws** on the provisional envelope (priced without the manifest), then the attempt signature (the provider's authority
//!    read verifies it).
//! 3. **Manifest** from every uncharged roster provider; each response verifies under that provider's proven key, answers only for
//!    that provider, names the session's root and this exact attempt, and all agree (manifest and prompt).
//! 4. **Signatures**: the exact Ready (with that manifest) in the request; each provider rebuilds, compares, durably publishes and
//!    signs; each signature verifies under its proven key over `Ready::id`.
use crate::attempt::NativeCustodyV3;
use crate::provider_transport_v1::{
    ProviderTransportPolicyV1, ProviderTransportPurposeV1, ProviderTransportRequestV1, ProviderTransportResponseV1,
    provider_transport_request_v1, provider_transport_verify_response_v1,
};
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_consensus_core::{
    BlockHash, Hash64,
    header::Header,
    palw_attempt_material_v3::{
        PALW_MATERIAL_READY_CONTEXT_V3, PalwAttemptMaterialEnvelopeV3, PalwNativeMaterialReadyV3,
        select_native_session_partial_view_v3, verify_ready_signature_v3,
    },
    palw_attempt_v2::{PalwAttemptUnsignedV2, attempt_id_v2},
    palw_material_capacity_v1::PalwMaterialManifestV1,
    palw_monetary_material_v1::PalwMonetaryMaterialRowV1,
    palw_state_v2::PalwBondKeyV2,
};
use std::collections::BTreeMap;
use std::io::{Read, Write};

/// Wire version of [`PalwNativeReadyRequestV3`] / [`PalwNativeReadyResponseV3`] / [`PalwNativeManifestResponseV3`].
pub const PALW_NATIVE_READY_EXCHANGE_VERSION_V3: u16 = 3;
/// One framed request or response; an unbounded length is refused (the provider service's cap).
pub const PALW_NATIVE_EXCHANGE_MAX_FRAME_V3: u64 = 1 << 30;

/// **What a producer sends its session's providers.** Everything a provider needs to derive the bundle ITSELF and to obtain
/// the material authority from its own Consensus API: the pre-entropy checkpoint (a direct parent of `header`), the candidate
/// header at the attempt's nonce, the envelope carrying the signed base attempt and the exact Ready (its provider signatures
/// and seal are placeholders — authority never reads them), and the capture. `prompt` is a cross-check only: a provider
/// re-derives the prompt from the anchor with its own backend and refuses a different one (a manifest query may leave it empty).
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct PalwNativeReadyRequestV3 {
    pub version: u16,
    pub checkpoint: BlockHash,
    pub header: Header,
    pub envelope: PalwAttemptMaterialEnvelopeV3,
    pub capture: Vec<u8>,
    pub prompt: Vec<u32>,
}

/// A provider's answer to `NativeReadyV3`: its signatures over `Ready::id` (each after durable publication), or why it signed
/// nothing.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub enum PalwNativeReadyResponseV3 {
    Signed { version: u16, signatures: Vec<(PalwBondKeyV2, Vec<u8>)> },
    Refused { version: u16, reason: String },
}

/// A provider's answer to `NativeManifestV3`: the one-scope manifest and the prompt it rebuilt from the capture under its own
/// node's authority (nothing published, nothing signed over a Ready), or why it answered nothing.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwNativeManifestResponseV3 {
    Manifest { version: u16, manifest: PalwMaterialManifestV1, prompt: Vec<u32> },
    Refused { version: u16, reason: String },
}

// The native wire scans (bounded preflight before Borsh allocates), moved with the types from kaspad.
impl kaspa_consensus_core::palw_native_resource_v1::NativeWireScanV1 for PalwNativeReadyRequestV3 {
    fn scan_native_wire_v1<R: std::io::Read>(
        r: &mut kaspa_consensus_core::palw_native_resource_v1::NativeWireReaderV1<R>,
    ) -> std::io::Result<kaspa_consensus_core::palw_native_resource_v1::NativeWireShapeV1> {
        use kaspa_consensus_core::palw_native_resource_v1::NativeWireScanV1;
        r.step()?;
        u16::scan_native_wire_v1(r)?
            .merge(BlockHash::scan_native_wire_v1(r)?)?
            .merge(Header::scan_native_wire_v1(r)?)?
            .merge(PalwAttemptMaterialEnvelopeV3::scan_native_wire_v1(r)?)?
            .merge(Vec::<u8>::scan_native_wire_v1(r)?)?
            .merge(Vec::<u32>::scan_native_wire_v1(r)?)
    }
}
impl kaspa_consensus_core::palw_native_resource_v1::NativeWireScanV1 for PalwNativeReadyResponseV3 {
    fn scan_native_wire_v1<R: std::io::Read>(
        r: &mut kaspa_consensus_core::palw_native_resource_v1::NativeWireReaderV1<R>,
    ) -> std::io::Result<kaspa_consensus_core::palw_native_resource_v1::NativeWireShapeV1> {
        use kaspa_consensus_core::palw_native_resource_v1::NativeWireScanV1;
        r.step()?;
        match r.byte()? {
            0 => u16::scan_native_wire_v1(r)?.merge(Vec::<(PalwBondKeyV2, Vec<u8>)>::scan_native_wire_v1(r)?),
            1 => u16::scan_native_wire_v1(r)?.merge(String::scan_native_wire_v1(r)?),
            _ => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "native Ready response tag unknown")),
        }
    }
}

impl kaspa_consensus_core::palw_native_resource_v1::NativeWireScanV1 for PalwNativeManifestResponseV3 {
    fn scan_native_wire_v1<R: std::io::Read>(
        r: &mut kaspa_consensus_core::palw_native_resource_v1::NativeWireReaderV1<R>,
    ) -> std::io::Result<kaspa_consensus_core::palw_native_resource_v1::NativeWireShapeV1> {
        use kaspa_consensus_core::palw_native_resource_v1::NativeWireScanV1;
        r.step()?;
        match r.byte()? {
            0 => u16::scan_native_wire_v1(r)?
                .merge(PalwMaterialManifestV1::scan_native_wire_v1(r)?)?
                .merge(Vec::<u32>::scan_native_wire_v1(r)?),
            1 => u16::scan_native_wire_v1(r)?.merge(String::scan_native_wire_v1(r)?),
            _ => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "native manifest response tag unknown")),
        }
    }
}

/// One frame: the length as 8 little-endian bytes, then the bytes (the provider service's framing).
pub fn write_native_frame_v3(stream: &mut impl Write, bytes: &[u8]) -> Result<(), String> {
    if bytes.len() as u64 > PALW_NATIVE_EXCHANGE_MAX_FRAME_V3 {
        return Err("native exchange frame exceeds its cap".into());
    }
    stream
        .write_all(&(bytes.len() as u64).to_le_bytes())
        .and_then(|_| stream.write_all(bytes))
        .and_then(|_| stream.flush())
        .map_err(|e| e.to_string())
}

/// One frame of at most `cap` bytes (and never past [`PALW_NATIVE_EXCHANGE_MAX_FRAME_V3`]); the length is checked before any
/// allocation.
pub fn read_native_frame_v3(stream: &mut impl Read, cap: u64) -> Result<Vec<u8>, String> {
    let mut length = [0u8; 8];
    stream.read_exact(&mut length).map_err(|e| e.to_string())?;
    let length = u64::from_le_bytes(length);
    if length > cap || length > PALW_NATIVE_EXCHANGE_MAX_FRAME_V3 {
        return Err("native exchange frame exceeds its cap".into());
    }
    let mut bytes = vec![0u8; length as usize];
    stream.read_exact(&mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes)
}

/// One provider as the miner was configured with it, its key PROVEN against a pin by the caller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderEndpointV3 {
    pub bond: PalwBondKeyV2,
    /// `tcp://<host>:<port>`: the provider service's framed socket.
    pub url: String,
    /// The registered ML-DSA-87 key (proven: never a key the provider's response names).
    pub pubkey: Vec<u8>,
    /// The registered operator id of the bond (from the same proof).
    pub operator_id: Hash64,
}

/// The wire between the miner and one provider.
pub trait ProviderLinkV3 {
    fn round_trip(
        &mut self,
        provider: &ProviderEndpointV3,
        request: &ProviderTransportRequestV1,
    ) -> Result<ProviderTransportResponseV1, String>;
}

/// [`ProviderLinkV3`] over the provider service's framed socket: one request per connection.
pub struct TcpProviderLinkV3 {
    pub max_response: u64,
    pub timeout: std::time::Duration,
}

impl ProviderLinkV3 for TcpProviderLinkV3 {
    fn round_trip(
        &mut self,
        provider: &ProviderEndpointV3,
        request: &ProviderTransportRequestV1,
    ) -> Result<ProviderTransportResponseV1, String> {
        use std::net::ToSocketAddrs;
        let addr = provider
            .url
            .strip_prefix("tcp://")
            .ok_or_else(|| format!("{}: not a tcp://<host>:<port> provider address", provider.url))?;
        let addr = addr
            .to_socket_addrs()
            .map_err(|e| format!("{}: {e}", provider.url))?
            .next()
            .ok_or("the provider address resolves to nothing")?;
        let mut stream = std::net::TcpStream::connect_timeout(&addr, self.timeout).map_err(|e| format!("{}: {e}", provider.url))?;
        stream.set_read_timeout(Some(self.timeout)).map_err(|e| e.to_string())?;
        stream.set_write_timeout(Some(self.timeout)).map_err(|e| e.to_string())?;
        write_native_frame_v3(&mut stream, &borsh::to_vec(request).map_err(|e| e.to_string())?)?;
        let frame = read_native_frame_v3(&mut stream, self.max_response)?;
        borsh::from_slice(&frame).map_err(|e| format!("{}: undecodable response: {e}", provider.url))
    }
}

pub type SignFnV3 = Box<dyn Fn(&[u8], &[u8]) -> Result<Vec<u8>, String>>;

/// The producer's [`NativeCustodyV3`] over configured, proven providers.
pub struct ProviderExchangeV3 {
    pub domain: Hash64,
    pub sender: PalwBondKeyV2,
    pub sender_pubkey: Vec<u8>,
    providers: BTreeMap<PalwBondKeyV2, ProviderEndpointV3>,
    rows: Vec<PalwMonetaryMaterialRowV1>,
    daa: u64,
    policy: ProviderTransportPolicyV1,
    link: Box<dyn ProviderLinkV3>,
    sign: SignFnV3,
    now_ms: Box<dyn Fn() -> u64>,
    counter: u64,
    /// Whether consensus's release-recovery filter (`palw_material_release_recovery_v2`) is in force at a DAA: the row view
    /// cannot evaluate it, so the session read holds there. Refusing (`|_| true`) until the caller names the fence.
    recovery_in_force: Box<dyn Fn(u64) -> bool>,
}

impl ProviderExchangeV3 {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        domain: Hash64,
        sender: PalwBondKeyV2,
        sender_pubkey: Vec<u8>,
        providers: Vec<ProviderEndpointV3>,
        max_body: u64,
        link: Box<dyn ProviderLinkV3>,
        sign: SignFnV3,
        now_ms: Box<dyn Fn() -> u64>,
    ) -> Result<Self, String> {
        let mut map = BTreeMap::new();
        for p in providers {
            if map.insert(p.bond, p).is_some() {
                return Err("a provider bond is configured twice".into());
            }
        }
        if map.is_empty() {
            return Err("no provider is configured".into());
        }
        Ok(Self {
            domain,
            sender,
            sender_pubkey,
            providers: map,
            rows: Vec::new(),
            daa: 0,
            policy: ProviderTransportPolicyV1::new(domain, max_body),
            link,
            sign,
            now_ms,
            counter: 0,
            recovery_in_force: Box::new(|_| true),
        })
    }

    /// Name where the release-recovery fence is in force (the binary passes its compiled params' fence). Until this is called
    /// the session read holds everywhere.
    pub fn with_recovery_fence(mut self, recovery_in_force: Box<dyn Fn(u64) -> bool>) -> Self {
        self.recovery_in_force = recovery_in_force;
        self
    }

    /// The authenticated row view for this step and the DAA score it was read at.
    pub fn set_rows(&mut self, rows: Vec<PalwMonetaryMaterialRowV1>, daa: u64) {
        self.rows = rows;
        self.daa = daa;
    }

    fn roster_keys(&self) -> Vec<(PalwBondKeyV2, Vec<u8>)> {
        self.providers.values().map(|p| (p.bond, p.pubkey.clone())).collect()
    }

    fn nonce(&mut self, issued: u64, body: &[u8]) -> [u8; 32] {
        self.counter += 1;
        let mut state = blake2b_simd::Params::new().hash_length(32).key(b"MISAKA/PALW/NATIVE-READY-EXCHANGE/NONCE/V3").to_state();
        state.update(&self.sender_pubkey);
        state.update(&issued.to_le_bytes());
        state.update(&self.counter.to_le_bytes());
        state.update(body);
        let mut out = [0u8; 32];
        out.copy_from_slice(state.finalize().as_bytes());
        if out == [0u8; 32] {
            out[0] = 1;
        }
        out
    }

    /// One signed request to ONE provider bond (the only recipient, so only it answers); the verified response body.
    fn ask(
        &mut self,
        bond: &PalwBondKeyV2,
        purpose: ProviderTransportPurposeV1,
        request: &PalwNativeReadyRequestV3,
    ) -> Result<Vec<u8>, String> {
        let provider = self
            .providers
            .get(bond)
            .cloned()
            .ok_or_else(|| format!("roster provider {}:{} has no configured endpoint", bond.0.transaction_id, bond.0.index))?;
        let bytes = borsh::to_vec(request).map_err(|e| e.to_string())?;
        let issued = (self.now_ms)();
        let nonce = self.nonce(issued, &bytes);
        let sign = &self.sign;
        let signed = provider_transport_request_v1(
            purpose,
            self.domain,
            self.sender,
            self.sender_pubkey.clone(),
            vec![*bond],
            nonce,
            issued,
            request.header.daa_score,
            bytes,
            |m, c| sign(m, c),
        )?;
        let response = self.link.round_trip(&provider, &signed)?;
        let responders =
            provider_transport_verify_response_v1(&response, &signed.header, &self.roster_keys(), &self.policy, (self.now_ms)())?;
        if responders != vec![*bond] {
            return Err("the response is signed by a party other than the provider asked".into());
        }
        Ok(response.body)
    }
}

/// The uncharged roster of a session row, in row order (the order `draft_native_ready_v3` uses).
pub fn session_roster_v3(row: &PalwMonetaryMaterialRowV1) -> Vec<PalwBondKeyV2> {
    row.providers.iter().filter(|p| !p.charged).map(|p| p.provider).collect()
}

impl NativeCustodyV3 for ProviderExchangeV3 {
    /// Audit C-02: the miner knows only its configured providers' operators, so it runs the partial-view form of consensus's
    /// rule (`select_native_session_partial_view_v3`): it selects the canonical row `verify_prior` re-selects, or HOLDS when a
    /// smaller-root row's health is undeterminable from here or the canonical roster names a provider it cannot reach. Past the
    /// release-recovery fence consensus also filters rows by their recovery enrollment, which the row view does not carry, so
    /// the miner holds there by name (`recovery_in_force`).
    fn session(&self, attempt: &PalwAttemptUnsignedV2, daa: u64) -> Option<PalwMonetaryMaterialRowV1> {
        if (self.recovery_in_force)(daa) {
            return None;
        }
        select_native_session_partial_view_v3(self.rows.iter().cloned(), attempt, daa, |bond| {
            self.providers.get(bond).map(|p| p.operator_id)
        })
    }

    fn manifest(
        &mut self,
        session: &PalwMonetaryMaterialRowV1,
        request: &PalwNativeReadyRequestV3,
    ) -> Result<(PalwMaterialManifestV1, Vec<u32>), String> {
        let roster = session_roster_v3(session);
        if roster.is_empty() {
            return Err("the session has no uncharged provider".into());
        }
        let attempt = &request.envelope.base.attempt;
        let claim = attempt_id_v2(attempt);
        let mut agreed: Option<(PalwMaterialManifestV1, Vec<u32>)> = None;
        for bond in &roster {
            let body = self.ask(bond, ProviderTransportPurposeV1::NativeManifestV3, request)?;
            let (manifest, prompt) = match borsh::from_slice::<PalwNativeManifestResponseV3>(&body)
                .map_err(|e| format!("undecodable manifest answer: {e}"))?
            {
                PalwNativeManifestResponseV3::Manifest { version: PALW_NATIVE_READY_EXCHANGE_VERSION_V3, manifest, prompt } => {
                    (manifest, prompt)
                }
                PalwNativeManifestResponseV3::Refused { reason, .. } => {
                    return Err(format!("provider {}:{} refused: {reason}", bond.0.transaction_id, bond.0.index));
                }
                PalwNativeManifestResponseV3::Manifest { .. } => return Err("unknown native manifest answer version".into()),
            };
            if manifest.root != session.terms.root
                || manifest.scopes.len() != 1
                || manifest.scopes[0].claim != claim
                || manifest.scopes[0].commitment != attempt.execution_root
            {
                return Err("a provider's manifest is not the session's one-scope manifest of this attempt".into());
            }
            match &agreed {
                None => agreed = Some((manifest, prompt)),
                Some(first) if *first == (manifest.clone(), prompt.clone()) => {}
                Some(_) => return Err("roster providers rebuilt different manifests or prompts from the same capture".into()),
            }
        }
        agreed.ok_or_else(|| "no manifest".into())
    }

    fn collect(&mut self, request: &PalwNativeReadyRequestV3) -> Result<BTreeMap<PalwBondKeyV2, Vec<u8>>, String> {
        let ready: &PalwNativeMaterialReadyV3 = &request.envelope.ready;
        let id = ready.id();
        let mut out = BTreeMap::new();
        for bond in &ready.providers {
            let body = self.ask(bond, ProviderTransportPurposeV1::NativeReadyV3, request)?;
            let signatures =
                match borsh::from_slice::<PalwNativeReadyResponseV3>(&body).map_err(|e| format!("undecodable sign answer: {e}"))? {
                    PalwNativeReadyResponseV3::Signed { version: PALW_NATIVE_READY_EXCHANGE_VERSION_V3, signatures } => signatures,
                    PalwNativeReadyResponseV3::Refused { reason, .. } => {
                        return Err(format!("provider {}:{} refused: {reason}", bond.0.transaction_id, bond.0.index));
                    }
                    PalwNativeReadyResponseV3::Signed { .. } => return Err("unknown native exchange answer version".into()),
                };
            // A provider service holding several roster bonds signs for each of them; only the asked bond's signature is taken.
            let signature = signatures
                .into_iter()
                .find(|(b, _)| b == bond)
                .map(|(_, s)| s)
                .ok_or("the provider's answer carries no signature of the asked bond")?;
            let key = &self.providers.get(bond).ok_or("roster provider vanished")?.pubkey;
            if !verify_ready_signature_v3(key, id.as_byte_slice(), &signature, PALW_MATERIAL_READY_CONTEXT_V3) {
                return Err(format!(
                    "provider {}:{} signature over Ready does not verify under its registered key",
                    bond.0.transaction_id, bond.0.index
                ));
            }
            out.insert(*bond, signature);
        }
        Ok(out)
    }

    fn seal(&self, message: &[u8], context: &[u8]) -> Result<Vec<u8>, String> {
        (self.sign)(message, context)
    }
}

/// One `--native-provider` flag: `<bond txid>:<index>=tcp://<host>:<port>=<pubkey hex | @file holding the hex>` (the provider
/// service's authenticated framed socket, audit C-03). The key named here is a CLAIM
/// the caller must prove against a pinned bond table before it is used ([`ProviderEndpointV3::pubkey`] is the proven key).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderFlagV3 {
    pub bond: PalwBondKeyV2,
    pub url: String,
    pub pubkey: ProviderKeySourceV3,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProviderKeySourceV3 {
    Hex(Vec<u8>),
    File(String),
}

pub fn parse_provider_flag_v3(text: &str) -> Result<ProviderFlagV3, String> {
    let mut parts = text.splitn(3, '=');
    let (bond, url, key) = match (parts.next(), parts.next(), parts.next()) {
        (Some(b), Some(u), Some(k)) if !b.is_empty() && !u.is_empty() && !k.is_empty() => (b, u, k),
        _ => return Err(format!("--native-provider {text:?} is not <txid>:<index>=tcp://<host>:<port>=<pubkey hex|@file>")),
    };
    let (txid, index) = bond.rsplit_once(':').ok_or("--native-provider bond is not <txid>:<index>")?;
    let txid: Hash64 = txid.trim().parse().map_err(|_| "--native-provider bond txid is not a 128-hex hash".to_string())?;
    let index: u32 = index.parse().map_err(|_| "--native-provider bond index is not a number".to_string())?;
    // Audit C-03: the one canonical provider wire is the provider service's length-framed authenticated socket.
    if url
        .strip_prefix("tcp://")
        .is_none_or(|addr| addr.rsplit_once(':').is_none_or(|(host, port)| host.is_empty() || port.parse::<u16>().is_err()))
    {
        return Err("--native-provider address must be tcp://<host>:<port> (the provider service's framed socket)".into());
    }
    let pubkey = match key.strip_prefix('@') {
        Some(path) => ProviderKeySourceV3::File(path.to_string()),
        None => ProviderKeySourceV3::Hex(decode_hex_v3(key)?),
    };
    Ok(ProviderFlagV3 {
        bond: PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(txid, index)),
        url: url.to_string(),
        pubkey,
    })
}

pub fn decode_hex_v3(text: &str) -> Result<Vec<u8>, String> {
    let t = text.trim();
    if t.len() % 2 != 0 || t.is_empty() {
        return Err("a provider key is not even-length hex".into());
    }
    (0..t.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&t[i..i + 2], 16).map_err(|_| "a provider key is not hex".to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider_transport_v1::{
        ProviderTransportReplayGuardV1, provider_transport_response_v1, provider_transport_verify_request_v1,
    };
    use kaspa_consensus_core::palw_attempt_material_v3::PalwNativeMaterialSessionV3;
    use kaspa_consensus_core::palw_attempt_v2::PalwAttemptEnvelopeV2;
    use kaspa_consensus_core::tx::TransactionOutpoint;
    use std::sync::{Arc, Mutex};

    type Kp = libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair;

    fn bond(word: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(word), 0))
    }
    fn key(seed: u8) -> Kp {
        libcrux_ml_dsa::ml_dsa_87::generate_key_pair([seed; 32])
    }
    fn sign_with(k: &Kp, m: &[u8], c: &[u8]) -> Result<Vec<u8>, String> {
        libcrux_ml_dsa::ml_dsa_87::sign(&k.signing_key, m, c, [7u8; 32]).map(|s| s.as_ref().to_vec()).map_err(|e| format!("{e:?}"))
    }
    const DOMAIN: u64 = 7;
    const DAA: u64 = 100;

    /// What the local provider server does with each request.
    #[derive(Clone, Copy, PartialEq, Debug)]
    enum Behaviour {
        Honest,
        OtherManifest,
        BadReadySignature,
        WrongKey,
        Refuse,
    }

    /// **The provider service's wire, served from a real local socket**: one framed `ProviderTransportRequestV1` per connection,
    /// verified with the same transport rules (purpose, domain, signature, DAA window, replay), the body decoded as the canonical
    /// `PalwNativeReadyRequestV3`. A manifest query answers the manifest the provider "rebuilt" (this double's fixed bundle for the
    /// capture it was given); a Ready request is signed only when its manifest is that one. Every answer is signed by the asked bond.
    struct LocalProviderServer {
        addr: std::net::SocketAddr,
        seen: Arc<Mutex<Vec<(PalwBondKeyV2, ProviderTransportPurposeV1)>>>,
    }

    fn serve(
        providers: Vec<(PalwBondKeyV2, Arc<Kp>, Behaviour)>,
        manifest_for: Arc<dyn Fn(&PalwNativeReadyRequestV3) -> PalwMaterialManifestV1 + Send + Sync>,
    ) -> LocalProviderServer {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        std::thread::spawn(move || {
            let policy = ProviderTransportPolicyV1::new(Hash64::from_u64_word(DOMAIN), 1 << 24);
            let mut guard = ProviderTransportReplayGuardV1::new(1024, policy.max_skew_ms);
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let Ok(frame) = read_native_frame_v3(&mut stream, 1 << 24) else { continue };
                let Ok(request) = borsh::from_slice::<ProviderTransportRequestV1>(&frame) else { continue };
                let purpose = request.header.purpose;
                if provider_transport_verify_request_v1(&request, purpose, &policy, 1_000, Some(DAA), &mut guard).is_err() {
                    continue; // unauthenticated bytes get no answer
                }
                let Ok(native) = borsh::from_slice::<PalwNativeReadyRequestV3>(&request.body) else { continue };
                let asked = request.header.recipients[0];
                let Some((_, k, behaviour)) = providers.iter().find(|(b, _, _)| *b == asked).cloned() else { continue };
                log.lock().unwrap().push((asked, purpose));
                let expected = manifest_for(&native);
                let body = match (purpose, behaviour) {
                    (_, Behaviour::Refuse) => {
                        borsh::to_vec(&PalwNativeManifestResponseV3::Refused { version: 3, reason: "busy".into() }).unwrap()
                    }
                    (ProviderTransportPurposeV1::NativeManifestV3, b) => {
                        let mut m = expected;
                        if b == Behaviour::OtherManifest {
                            m.scopes[0].units += 1;
                        }
                        borsh::to_vec(&PalwNativeManifestResponseV3::Manifest { version: 3, manifest: m, prompt: vec![1, 2, 3] })
                            .unwrap()
                    }
                    (ProviderTransportPurposeV1::NativeReadyV3, b) => {
                        let ready = &native.envelope.ready;
                        let answer = if ready.binding.manifest != expected || native.prompt != vec![1, 2, 3] {
                            PalwNativeReadyResponseV3::Refused { version: 3, reason: "the producer's manifest differs".into() }
                        } else {
                            let message = if b == Behaviour::BadReadySignature { Hash64::from_u64_word(99) } else { ready.id() };
                            let sig = sign_with(&k, message.as_byte_slice(), PALW_MATERIAL_READY_CONTEXT_V3).unwrap();
                            PalwNativeReadyResponseV3::Signed { version: 3, signatures: vec![(asked, sig)] }
                        };
                        borsh::to_vec(&answer).unwrap()
                    }
                    _ => continue,
                };
                let signer_key = if behaviour == Behaviour::WrongKey { Arc::new(key(200)) } else { k.clone() };
                let s = move |m: &[u8], c: &[u8]| sign_with(&signer_key, m, c);
                let s: &dyn Fn(&[u8], &[u8]) -> Result<Vec<u8>, String> = &s;
                let Ok(response) = provider_transport_response_v1(&request.header, 1_000, body, vec![(asked, s)]) else { continue };
                let _ = write_native_frame_v3(&mut stream, &borsh::to_vec(&response).unwrap());
            }
        });
        LocalProviderServer { addr, seen }
    }

    fn zeroed<T: BorshDeserialize>() -> T {
        (0..65536).find_map(|len| borsh::from_slice::<T>(&vec![0u8; len]).ok()).expect("a zeroed value decodes")
    }

    fn row_for(roster: &[PalwBondKeyV2], root: Hash64) -> PalwMonetaryMaterialRowV1 {
        let mut row: PalwMonetaryMaterialRowV1 = zeroed();
        row.terms.root = root;
        row.terms.max_material_bytes = 1 << 20;
        row.consent = Hash64::from_u64_word(0xC5);
        row.providers = roster
            .iter()
            .map(|b| {
                let mut p: kaspa_consensus_core::palw_monetary_material_v1::PalwMonetaryMaterialProviderV1 = zeroed();
                p.provider = *b;
                p
            })
            .collect();
        row
    }

    fn session_of(a: &PalwAttemptUnsignedV2) -> PalwNativeMaterialSessionV3 {
        PalwNativeMaterialSessionV3 {
            version: 3,
            class_id: a.class_id,
            artifact_root: a.artifact_root,
            executor: PalwBondKeyV2(a.executor_bond),
            operator: a.operator_id,
            job_policy: kaspa_consensus_core::palw_attempt_material_v3::native_job_policy_v3(),
            codec: kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialCodecV1::NativeHeldComplete,
            max_units: 64,
            one_claim_cap: 1,
            admit_from_daa: 0,
            admit_until_daa: 1_000,
        }
    }

    /// The bundle every honest provider rebuilds for a request: the session's root, this attempt, 64 bytes, 1 unit.
    fn rebuilt(request: &PalwNativeReadyRequestV3) -> PalwMaterialManifestV1 {
        let a = &request.envelope.base.attempt;
        PalwMaterialManifestV1 {
            root: request.envelope.ready.session,
            scopes: vec![kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialScopeV1 {
                claim: attempt_id_v2(a),
                commitment: a.execution_root,
                codec: kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialCodecV1::NativeHeldComplete,
                bytes: request.capture.len() as u64,
                units: 1,
                content: Hash64::from_u64_word(0xC0),
            }],
        }
    }

    struct Fixture {
        exchange: ProviderExchangeV3,
        bonds: Vec<PalwBondKeyV2>,
        server: LocalProviderServer,
        executor: Arc<Kp>,
    }

    fn fixture(behaviours: &[Behaviour]) -> Fixture {
        let executor = Arc::new(key(1));
        let mut served = Vec::new();
        let mut keys = Vec::new();
        for (i, b) in behaviours.iter().enumerate() {
            let k = Arc::new(key(10 + i as u8));
            served.push((bond(100 + i as u64), k.clone(), *b));
            keys.push(k);
        }
        let server = serve(served.clone(), Arc::new(rebuilt));
        let endpoints = served
            .iter()
            .enumerate()
            .map(|(i, (b, k, _))| ProviderEndpointV3 {
                bond: *b,
                url: format!("tcp://{}", server.addr),
                pubkey: k.verification_key.as_ref().to_vec(),
                operator_id: Hash64::from_u64_word(500 + i as u64),
            })
            .collect();
        let ex = executor.clone();
        let exchange = ProviderExchangeV3::new(
            Hash64::from_u64_word(DOMAIN),
            bond(1),
            executor.verification_key.as_ref().to_vec(),
            endpoints,
            1 << 24,
            Box::new(TcpProviderLinkV3 { max_response: 1 << 24, timeout: std::time::Duration::from_secs(20) }),
            Box::new(move |m, c| sign_with(&ex, m, c)),
            Box::new(|| 1_000),
        )
        .unwrap()
        .with_recovery_fence(Box::new(|_| false));
        Fixture { exchange, bonds: served.iter().map(|(b, _, _)| *b).collect(), server, executor }
    }

    fn header(daa: u64) -> Header {
        Header::new_finalized(
            1,
            vec![vec![Hash64::from_u64_word(5)]].try_into().unwrap(),
            Hash64::from_u64_word(2),
            Hash64::from_u64_word(3),
            Hash64::from_u64_word(4),
            1_700_000,
            0x1d00ffff,
            0,
            kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2,
            daa,
            0u64.into(),
            0,
            Hash64::from_u64_word(6),
        )
    }

    /// A canonical request over `roster` and `root`: the draw envelope (placeholder manifest) on a zeroed attempt.
    fn request(roster: &[PalwBondKeyV2], root: Hash64, executor: &Kp) -> (PalwNativeReadyRequestV3, PalwMonetaryMaterialRowV1) {
        let mut a: PalwAttemptUnsignedV2 = zeroed();
        a.executor_bond = bond(1).0;
        a.executor_pubkey = executor.verification_key.as_ref().to_vec();
        a.execution_root = Hash64::from_u64_word(0xE1);
        let mut row = row_for(roster, root);
        row.terms.native_session = Some(session_of(&a));
        let draw = kaspa_consensus_core::palw_attempt_material_v3::palw_native_draw_manifest_v3(&row, &a).unwrap();
        let ready =
            kaspa_consensus_core::palw_attempt_material_v3::draft_native_ready_v3(&row, &a, Hash64::from_u64_word(9), draw).unwrap();
        let dummy = vec![0u8; kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN];
        let envelope = PalwAttemptMaterialEnvelopeV3 {
            version: 3,
            base: PalwAttemptEnvelopeV2 { attempt: a, signature: dummy.clone() },
            provider_signatures: vec![dummy.clone(); ready.providers.len()],
            ready,
            seal: dummy,
        };
        let request = PalwNativeReadyRequestV3 {
            version: PALW_NATIVE_READY_EXCHANGE_VERSION_V3,
            checkpoint: Hash64::from_u64_word(5),
            header: header(DAA),
            envelope,
            capture: vec![7; 64],
            prompt: Vec::new(),
        };
        (request, row)
    }

    /// Audit C-03: the miner and the provider speak ONE wire — the provider service's framed authenticated socket and its
    /// request body — and the manifest every roster provider rebuilt comes back verified and agreed, with the prompt.
    #[test]
    fn audit_fixcd_c03_the_manifest_round_runs_over_the_providers_framed_socket() {
        let mut f = fixture(&[Behaviour::Honest, Behaviour::Honest]);
        let (req, row) = request(&f.bonds, Hash64::from_u64_word(42), &f.executor);
        let (manifest, prompt) = f.exchange.manifest(&row, &req).expect("both providers answer one manifest");
        assert_eq!(manifest, rebuilt(&req));
        assert_eq!(prompt, vec![1, 2, 3]);
        let seen = f.server.seen.lock().unwrap().clone();
        assert_eq!(seen.len(), 2);
        assert!(seen.iter().all(|(_, p)| *p == ProviderTransportPurposeV1::NativeManifestV3));
    }

    #[test]
    fn audit_fixcd_c03_disagreeing_refusing_or_foreign_providers_mount_nothing() {
        let mut f = fixture(&[Behaviour::Honest, Behaviour::OtherManifest]);
        let (req, row) = request(&f.bonds, Hash64::from_u64_word(42), &f.executor);
        assert!(f.exchange.manifest(&row, &req).unwrap_err().contains("different manifests"));
        let mut f = fixture(&[Behaviour::Honest, Behaviour::Refuse]);
        let (req, row) = request(&f.bonds, Hash64::from_u64_word(42), &f.executor);
        assert!(f.exchange.manifest(&row, &req).unwrap_err().contains("refused"));
        let mut f = fixture(&[Behaviour::WrongKey, Behaviour::Honest]);
        let (req, row) = request(&f.bonds, Hash64::from_u64_word(42), &f.executor);
        assert!(f.exchange.manifest(&row, &req).is_err(), "an answer not signed by the proven key");
        let mut f = fixture(&[Behaviour::Honest]);
        let mut roster = f.bonds.clone();
        roster.push(bond(999));
        let (req, row) = request(&roster, Hash64::from_u64_word(42), &f.executor);
        assert!(f.exchange.manifest(&row, &req).unwrap_err().contains("no configured endpoint"));
    }

    #[test]
    fn audit_fixcd_c03_ready_signatures_come_back_verified_over_the_socket() {
        let mut f = fixture(&[Behaviour::Honest, Behaviour::Honest]);
        let (mut req, row) = request(&f.bonds, Hash64::from_u64_word(42), &f.executor);
        let a = req.envelope.base.attempt.clone();
        req.envelope.ready =
            kaspa_consensus_core::palw_attempt_material_v3::draft_native_ready_v3(&row, &a, Hash64::from_u64_word(9), rebuilt(&req))
                .unwrap();
        // Without the prompt the provider derived: refused (the provider re-derives and compares).
        assert!(f.exchange.collect(&req).unwrap_err().contains("refused"));
        req.prompt = vec![1, 2, 3];
        let sigs = f.exchange.collect(&req).unwrap();
        assert_eq!(sigs.keys().copied().collect::<Vec<_>>(), f.bonds);
        let mut f = fixture(&[Behaviour::Honest, Behaviour::BadReadySignature]);
        let (mut req, row) = request(&f.bonds, Hash64::from_u64_word(42), &f.executor);
        req.envelope.ready =
            kaspa_consensus_core::palw_attempt_material_v3::draft_native_ready_v3(&row, &a, Hash64::from_u64_word(9), rebuilt(&req))
                .unwrap();
        req.prompt = vec![1, 2, 3];
        assert!(f.exchange.collect(&req).unwrap_err().contains("does not verify"));
    }

    /// **End to end (audit C-03 + C-05)**: `mount_native_attempt_v3` through the exchange over the local provider socket — draws on
    /// the draw envelope, the attempt signed, the manifest rebuilt by both providers, the exact Ready signed by both, the seal —
    /// and the mounted header's PAV3 carriage validates: shape, the executor seal under the attempt's key, every provider signature
    /// under its key over `Ready::id`, and the priced execution of the draw.
    #[test]
    fn audit_fixcd_c03_a_remote_native_attempt_mounts_end_to_end_over_the_canonical_wire() {
        use crate::attempt::{AttemptExecutor, AttemptParams, ExecutedAttempt, MlDsaAttemptSigner, mount_native_attempt_v3};
        use crate::template::{ProducerFactsSummary, TemplateObservation, TemplatePolicy, check_templates};
        use kaspa_consensus_core::palw_attempt_v2::PalwAttemptExecutionV1;
        let mut f = fixture(&[Behaviour::Honest, Behaviour::Honest]);
        struct Exec;
        impl AttemptExecutor for Exec {
            fn execute(&mut self, _anchor: Hash64) -> Result<ExecutedAttempt, String> {
                Ok(ExecutedAttempt {
                    execution: PalwAttemptExecutionV1 {
                        trace_root: Hash64::from_u64_word(10),
                        output_root: Hash64::from_u64_word(11),
                        execution_root: Hash64::from_u64_word(12),
                        trace_manifest_root: Hash64::from_u64_word(13),
                        trace_chunk_count: 1,
                    },
                    material: vec![7; 64],
                })
            }
        }
        let signer = MlDsaAttemptSigner { keypair: key(1) };
        let params = AttemptParams {
            network_id: "testnet-12".into(),
            network_domain: Hash64::from_u64_word(DOMAIN),
            bond: bond(1).0,
            operator_id: Hash64::from_u64_word(22),
            class_target: u128::MAX,
            witness_chunks: 0,
            single_lottery: true,
            signature_len: kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN,
        };
        let (class_id, artifact_root) = (Hash64::from_u64_word(30), Hash64::from_u64_word(31));
        let mut row = row_for(&f.bonds, Hash64::from_u64_word(42));
        row.terms.native_session = Some(PalwNativeMaterialSessionV3 {
            version: 3,
            class_id,
            artifact_root,
            executor: bond(1),
            operator: params.operator_id,
            job_policy: kaspa_consensus_core::palw_attempt_material_v3::native_job_policy_v3(),
            codec: kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialCodecV1::NativeHeldComplete,
            max_units: 64,
            one_claim_cap: 1,
            admit_from_daa: 0,
            admit_until_daa: 1_000,
        });
        f.exchange.set_rows(vec![row], DAA);
        let obs = |node: &str| TemplateObservation {
            node: node.into(),
            network_id: "testnet-12".into(),
            pruning_point: Hash64::from_u64_word(1),
            version: 1,
            pow_algo_id: 6,
            bits: 0x1d00ffff,
            daa_score: DAA,
            palw_state_root: Hash64::from_u64_word(2),
            parents: vec![Hash64::from_u64_word(5)],
            facts: ProducerFactsSummary {
                chain_point: Hash64::from_u64_word(5),
                class_id,
                artifact_root,
                class_target: u128::MAX,
                pwu: 7_708,
                min_trace_retention_daa: 3_000,
                bond_pubkey: vec![9; 8],
                not_ready_reason: String::new(),
            },
        };
        let policy =
            TemplatePolicy { min_agree: 2, max_template_age_daa: 30, held_pubkey: vec![9; 8], held_artifact_root: artifact_root };
        let template = check_templates(&[obs("a"), obs("b")], &policy, DAA + 5).unwrap();
        let mounted = mount_native_attempt_v3(
            &template,
            &header(DAA),
            &params,
            7_708,
            3_000,
            artifact_root,
            class_id,
            &mut None,
            &mut Exec,
            &signer,
            &mut f.exchange,
            &|_, _, _| true,
        )
        .expect("mounts")
        .expect("both draws won");
        let envelope = PalwAttemptMaterialEnvelopeV3::decode_wire(&mounted.header.palw_commitment).expect("a PAV3 carriage");
        envelope.verify_seal(verify_ready_signature_v3).expect("the executor's seal verifies under the attempt key");
        let id = envelope.ready.id();
        for (provider, sig) in envelope.ready.providers.iter().zip(&envelope.provider_signatures) {
            let i = f.bonds.iter().position(|b| b == provider).unwrap();
            let k = key(10 + i as u8);
            assert!(verify_ready_signature_v3(k.verification_key.as_ref(), id.as_byte_slice(), sig, PALW_MATERIAL_READY_CONTEXT_V3));
        }
        assert_eq!(envelope.ready.binding.manifest.scopes[0].bytes, 64, "the providers' rebuilt manifest, not the draw placeholder");
        let purposes: Vec<_> = f.server.seen.lock().unwrap().iter().map(|(_, p)| *p).collect();
        assert_eq!(
            purposes,
            vec![
                ProviderTransportPurposeV1::NativeManifestV3,
                ProviderTransportPurposeV1::NativeManifestV3,
                ProviderTransportPurposeV1::NativeReadyV3,
                ProviderTransportPurposeV1::NativeReadyV3
            ]
        );
        // The template's chain point must be a direct parent of the header (the provider's checkpoint): otherwise nothing mounts.
        let mut other = header(DAA);
        other.parents_by_level = vec![vec![Hash64::from_u64_word(77)]].try_into().unwrap();
        assert!(
            mount_native_attempt_v3(
                &template,
                &other,
                &params,
                7_708,
                3_000,
                artifact_root,
                class_id,
                &mut None,
                &mut Exec,
                &signer,
                &mut f.exchange,
                &|_, _, _| true
            )
            .is_err()
        );
    }

    #[test]
    fn native_provider_flag_parses_the_framed_socket_address_and_refuses_malformed_text() {
        let txid = Hash64::from_u64_word(9).to_string();
        let f = parse_provider_flag_v3(&format!("{txid}:2=tcp://10.0.0.1:7000=0aff")).unwrap();
        assert_eq!(f.bond, PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(9), 2)));
        assert_eq!(f.url, "tcp://10.0.0.1:7000");
        assert_eq!(f.pubkey, ProviderKeySourceV3::Hex(vec![0x0a, 0xff]));
        assert_eq!(
            parse_provider_flag_v3(&format!("{txid}:0=tcp://p:1=@/k.hex")).unwrap().pubkey,
            ProviderKeySourceV3::File("/k.hex".into())
        );
        for bad in [
            format!("{txid}:0=tcp://p:1"),
            format!("{txid}=tcp://p:1=00"),
            format!("{txid}:0=http://p:1=00"),
            format!("{txid}:0=tcp://p=00"),
            format!("{txid}:0=tcp://p:1=0g"),
        ] {
            assert!(parse_provider_flag_v3(&bad).is_err(), "{bad}");
        }
    }

    /// **audit(c) C-02 reproducer** (verify-3's corrected form): consensus re-selects the canonical session with the operator of
    /// EVERY bond on chain; the remote miner knows only its configured providers. A smaller-root session whose providers the miner
    /// does not know is healthy on chain; the miner must select consensus's canonical row or hold — never a later one.
    #[test]
    fn audit_c02_remote_miner_session_choice_diverges_from_consensus_with_partial_provider_config() {
        let mut f = fixture(&[Behaviour::Honest, Behaviour::Honest]);
        let (req, _) = request(&f.bonds, Hash64::from_u64_word(2), &f.executor);
        let a = req.envelope.base.attempt.clone();
        let daa = 10;
        let session = session_of(&a);
        assert!(session.matches(&a, daa));
        let unknown = [bond(900), bond(901)];
        let mut row_a = row_for(&unknown, Hash64::from_u64_word(1));
        row_a.terms.native_session = Some(session.clone());
        let mut row_b = row_for(&f.bonds, Hash64::from_u64_word(2));
        row_b.terms.native_session = Some(session);
        let rows = vec![row_a.clone(), row_b.clone()];
        let chain_operator = |b: &PalwBondKeyV2| Some(b.0.transaction_id);
        let consensus =
            kaspa_consensus_core::palw_attempt_material_v3::select_native_session_v3(rows.clone(), &a, daa, chain_operator)
                .expect("consensus selects a session");
        assert_eq!(consensus.terms.root, row_a.terms.root, "consensus picks the canonical smaller-root session");
        f.exchange.set_rows(rows, daa);
        let miner_root = f.exchange.session(&a, daa).map(|r| r.terms.root);
        assert!(
            miner_root.is_none() || miner_root == Some(consensus.terms.root),
            "C-02: remote miner selected session {:?}, consensus verify_prior requires {:?} (row B={:?})",
            miner_root,
            consensus.terms.root,
            row_b.terms.root
        );
    }

    /// Audit C-02: with every roster provider proven, the miner selects consensus's canonical row; past the release-recovery
    /// fence (which the row view cannot evaluate) it holds by name, and an exchange never told where that fence is holds too.
    #[test]
    fn audit_fixcd_c02_the_miner_selects_the_canonical_row_and_holds_where_it_cannot_know_it() {
        let f = fixture(&[Behaviour::Honest, Behaviour::Honest]);
        let (req, _) = request(&f.bonds, Hash64::from_u64_word(2), &f.executor);
        let a = req.envelope.base.attempt.clone();
        let mut x = f.exchange;
        let mut lone = row_for(&[bond(900)], Hash64::from_u64_word(1));
        lone.terms.native_session = Some(session_of(&a));
        let mut ours = row_for(&f.bonds, Hash64::from_u64_word(2));
        ours.terms.native_session = Some(session_of(&a));
        x.set_rows(vec![lone, ours.clone()], 10);
        assert_eq!(x.session(&a, 10).map(|r| r.terms.root), Some(ours.terms.root), "a one-provider row is unhealthy on chain");
        let x = x.with_recovery_fence(Box::new(|daa| daa >= 10));
        assert!(x.session(&a, 10).is_none(), "past the recovery fence the row view cannot decide: hold");
        let mut unnamed = fixture(&[Behaviour::Honest, Behaviour::Honest]).exchange.with_recovery_fence(Box::new(|_| true));
        unnamed.set_rows(vec![ours], 10);
        assert!(unnamed.session(&a, 10).is_none());
    }
}
