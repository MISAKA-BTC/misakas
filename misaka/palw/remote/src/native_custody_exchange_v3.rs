//! **RFC-0009 × RFC-0008 PAV3: the producer's provider exchange for the shipping remote miner** (G14 lane rfc09-13-19,
//! 2026-10-22; written, not compiled here).
//!
//! [`crate::attempt::NativeCustodyV3`] was the seam; nothing in the shipping CLI implemented it, so past the
//! `palw_monetary_material_v1` fence `palw-remote-miner` stopped with `NativeCustodyRequired`. [`ProviderExchangeV3`] is that
//! implementation, in three parts, each refusing rather than degrading:
//!
//! 1. **Session open.** The caller refreshes the AUTHENTICATED row view ([`ProviderExchangeV3::set_rows`]: op 242 pages finished by
//!    `MonetaryMaterialProofPagerV1`) each step; [`NativeCustodyV3::session`] is `select_native_session_v3` over those rows, with
//!    `operator_of` answered ONLY for provider bonds whose registered key the caller proved against a pin (an unknown provider
//!    counts for nothing, so a session is never healthier here than on the chain).
//! 2. **Manifest fetch + verify.** Every uncharged roster provider is asked, over the signed provider transport
//!    ([`crate::provider_transport_v1`], purpose `NativeReadyV3`), for the one-scope manifest it rebuilds from the capture. Each
//!    response must verify under that provider's proven key, answer only for that provider, and every provider must return the
//!    SAME manifest whose root is the session's. One silent, disagreeing or unauthenticated provider mounts nothing.
//! 3. **Signature exchange.** After both draws are won, each roster provider is sent the Ready draft and the capture; its reply is
//!    its signature over `Ready::id`, accepted only if it verifies under its proven key (`verify_ready_signature_v3`).
//!
//! The provider's half (durable publish, then sign: [`crate::native_ready_v3::provider_sign_native_ready_v3`]) is the provider's
//! process. No body here is authority by itself: every value is re-checked against the chain-proven roster and session.
use crate::attempt::NativeCustodyV3;
use crate::provider_transport_v1::{
    ProviderTransportPolicyV1, ProviderTransportPurposeV1, ProviderTransportRequestV1, ProviderTransportResponseV1,
    provider_transport_request_v1, provider_transport_verify_response_v1,
};
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_consensus_core::{
    Hash64,
    palw_attempt_material_v3::{PALW_MATERIAL_READY_CONTEXT_V3, PalwNativeMaterialReadyV3, select_native_session_v3, verify_ready_signature_v3},
    palw_attempt_v2::PalwAttemptUnsignedV2,
    palw_material_capacity_v1::PalwMaterialManifestV1,
    palw_monetary_material_v1::PalwMonetaryMaterialRowV1,
    palw_state_v2::PalwBondKeyV2,
};
use std::collections::BTreeMap;

/// The HTTP path a provider serves the `NativeReadyV3` exchange on (POST, borsh `ProviderTransportRequestV1` in, borsh
/// `ProviderTransportResponseV1` out).
pub const PALW_NATIVE_READY_EXCHANGE_PATH_V3: &str = "/palw/native-ready/v3";

/// Body of a `NativeReadyV3` request (the type `ProviderTransportPurposeV1::NativeReadyV3` names).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwNativeReadyRequestV3 {
    /// Rebuild the typed bundle from `material` for `attempt` under `session` and return its one-scope manifest.
    Manifest { session: Hash64, attempt: PalwAttemptUnsignedV2, material: Vec<u8> },
    /// Durably publish the bundle `ready` cites, then sign `ready.id()`.
    Sign { ready: PalwNativeMaterialReadyV3, material: Vec<u8> },
}

/// Body of a `NativeReadyV3` response.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwNativeReadyResponseV3 {
    Manifest(PalwMaterialManifestV1),
    Signature(Vec<u8>),
    /// A typed refusal by the provider (never treated as success).
    Refused(String),
}

/// One provider as the miner was configured with it, its key PROVEN against a pin by the caller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderEndpointV3 {
    pub bond: PalwBondKeyV2,
    pub url: String,
    /// The registered ML-DSA-87 key (proven: never a key the provider's response names).
    pub pubkey: Vec<u8>,
    /// The registered operator id of the bond (from the same proof).
    pub operator_id: Hash64,
}

/// The wire between the miner and one provider.
pub trait ProviderLinkV3 {
    fn round_trip(&mut self, provider: &ProviderEndpointV3, request: &ProviderTransportRequestV1) -> Result<ProviderTransportResponseV1, String>;
}

/// [`ProviderLinkV3`] over the crate's HTTP transport.
pub struct HttpProviderLinkV3 {
    pub max_response: usize,
}

impl ProviderLinkV3 for HttpProviderLinkV3 {
    fn round_trip(&mut self, provider: &ProviderEndpointV3, request: &ProviderTransportRequestV1) -> Result<ProviderTransportResponseV1, String> {
        let url = format!("{}{}", provider.url.trim_end_matches('/'), PALW_NATIVE_READY_EXCHANGE_PATH_V3);
        let body = borsh::to_vec(request).map_err(|e| e.to_string())?;
        let response = crate::transport::transport_for(&url).request("POST", &url, Some(&body), self.max_response)?;
        if response.status != 200 {
            return Err(format!("{url}: HTTP {}", response.status));
        }
        borsh::from_slice(&response.body).map_err(|e| format!("{url}: undecodable response: {e}"))
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
        })
    }

    /// The authenticated row view for this step and the DAA score requests are about.
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

    /// One signed request to one provider; the verified response body.
    fn ask(&mut self, bond: &PalwBondKeyV2, body: &PalwNativeReadyRequestV3) -> Result<PalwNativeReadyResponseV3, String> {
        let provider = self
            .providers
            .get(bond)
            .cloned()
            .ok_or_else(|| format!("roster provider {}:{} has no configured endpoint", bond.0.transaction_id, bond.0.index))?;
        let bytes = borsh::to_vec(body).map_err(|e| e.to_string())?;
        let issued = (self.now_ms)();
        let nonce = self.nonce(issued, &bytes);
        let sign = &self.sign;
        let request = provider_transport_request_v1(
            ProviderTransportPurposeV1::NativeReadyV3,
            self.domain,
            self.sender,
            self.sender_pubkey.clone(),
            vec![*bond],
            nonce,
            issued,
            self.daa,
            bytes,
            |m, c| sign(m, c),
        )?;
        let response = self.link.round_trip(&provider, &request)?;
        let responders =
            provider_transport_verify_response_v1(&response, &request.header, &self.roster_keys(), &self.policy, (self.now_ms)())?;
        if responders != vec![*bond] {
            return Err("the response is signed by a party other than the provider asked".into());
        }
        let out: PalwNativeReadyResponseV3 = borsh::from_slice(&response.body).map_err(|e| format!("undecodable exchange body: {e}"))?;
        if let PalwNativeReadyResponseV3::Refused(why) = &out {
            return Err(format!("provider {}:{} refused: {why}", bond.0.transaction_id, bond.0.index));
        }
        Ok(out)
    }
}

/// The uncharged roster of a session row, in row order (the order `draft_native_ready_v3` uses).
pub fn session_roster_v3(row: &PalwMonetaryMaterialRowV1) -> Vec<PalwBondKeyV2> {
    row.providers.iter().filter(|p| !p.charged).map(|p| p.provider).collect()
}

impl NativeCustodyV3 for ProviderExchangeV3 {
    fn session(&self, attempt: &PalwAttemptUnsignedV2, daa: u64) -> Option<PalwMonetaryMaterialRowV1> {
        select_native_session_v3(self.rows.iter().cloned(), attempt, daa, |bond| self.providers.get(bond).map(|p| p.operator_id))
    }

    fn manifest(
        &mut self,
        session: &PalwMonetaryMaterialRowV1,
        attempt: &PalwAttemptUnsignedV2,
        material: &[u8],
    ) -> Result<PalwMaterialManifestV1, String> {
        let roster = session_roster_v3(session);
        if roster.is_empty() {
            return Err("the session has no uncharged provider".into());
        }
        let mut agreed: Option<PalwMaterialManifestV1> = None;
        for bond in &roster {
            let body = PalwNativeReadyRequestV3::Manifest { session: session.terms.root, attempt: attempt.clone(), material: material.to_vec() };
            let manifest = match self.ask(bond, &body)? {
                PalwNativeReadyResponseV3::Manifest(m) => m,
                _ => return Err("a provider answered a manifest request with another body".into()),
            };
            if manifest.root != session.terms.root || manifest.scopes.len() != 1 {
                return Err("a provider's manifest is not the session's one-scope manifest".into());
            }
            match &agreed {
                None => agreed = Some(manifest),
                Some(first) if *first == manifest => {}
                Some(_) => return Err("roster providers rebuilt different manifests from the same capture".into()),
            }
        }
        agreed.ok_or_else(|| "no manifest".into())
    }

    fn collect(&mut self, ready: &PalwNativeMaterialReadyV3, material: &[u8]) -> Result<BTreeMap<PalwBondKeyV2, Vec<u8>>, String> {
        let id = ready.id();
        let mut out = BTreeMap::new();
        for bond in &ready.providers {
            let body = PalwNativeReadyRequestV3::Sign { ready: ready.clone(), material: material.to_vec() };
            let signature = match self.ask(bond, &body)? {
                PalwNativeReadyResponseV3::Signature(s) => s,
                _ => return Err("a provider answered a sign request with another body".into()),
            };
            let key = &self.providers.get(bond).ok_or("roster provider vanished")?.pubkey;
            if !verify_ready_signature_v3(key, id.as_byte_slice(), &signature, PALW_MATERIAL_READY_CONTEXT_V3) {
                return Err(format!("provider {}:{} signature over Ready does not verify under its registered key", bond.0.transaction_id, bond.0.index));
            }
            out.insert(*bond, signature);
        }
        Ok(out)
    }

    fn seal(&self, message: &[u8], context: &[u8]) -> Result<Vec<u8>, String> {
        (self.sign)(message, context)
    }
}

/// One `--native-provider` flag: `<bond txid>:<index>=<url>=<pubkey hex | @file holding the hex>`. The key named here is a CLAIM
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
        _ => return Err(format!("--native-provider {text:?} is not <txid>:<index>=<url>=<pubkey hex|@file>")),
    };
    let (txid, index) = bond.rsplit_once(':').ok_or("--native-provider bond is not <txid>:<index>")?;
    let txid: Hash64 = txid.trim().parse().map_err(|_| "--native-provider bond txid is not a 128-hex hash".to_string())?;
    let index: u32 = index.parse().map_err(|_| "--native-provider bond index is not a number".to_string())?;
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("--native-provider url must be http:// or https://".into());
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
    (0..t.len()).step_by(2).map(|i| u8::from_str_radix(&t[i..i + 2], 16).map_err(|_| "a provider key is not hex".to_string())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider_transport_v1::provider_transport_response_v1;
    use kaspa_consensus_core::tx::TransactionOutpoint;
    use std::cell::RefCell;
    use std::rc::Rc;

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

    /// What a mock provider does with each request.
    #[derive(Clone, Copy, PartialEq)]
    enum Behaviour {
        Honest,
        OtherManifest,
        BadReadySignature,
        WrongKey,
    }

    struct MockProviders {
        keys: BTreeMap<PalwBondKeyV2, (Rc<Kp>, Behaviour)>,
        manifest: PalwMaterialManifestV1,
        seen: Rc<RefCell<Vec<(PalwBondKeyV2, &'static str)>>>,
    }

    impl ProviderLinkV3 for MockProviders {
        fn round_trip(&mut self, provider: &ProviderEndpointV3, request: &ProviderTransportRequestV1) -> Result<ProviderTransportResponseV1, String> {
            let (k, behaviour) = self.keys.get(&provider.bond).cloned().ok_or("unknown provider")?;
            let body: PalwNativeReadyRequestV3 = borsh::from_slice(&request.body).map_err(|e| e.to_string())?;
            let answer = match body {
                PalwNativeReadyRequestV3::Manifest { .. } => {
                    self.seen.borrow_mut().push((provider.bond, "manifest"));
                    let mut m = self.manifest.clone();
                    if behaviour == Behaviour::OtherManifest {
                        m.scopes[0].units += 1;
                    }
                    PalwNativeReadyResponseV3::Manifest(m)
                }
                PalwNativeReadyRequestV3::Sign { ready, .. } => {
                    self.seen.borrow_mut().push((provider.bond, "sign"));
                    let message = if behaviour == Behaviour::BadReadySignature { Hash64::from_u64_word(99) } else { ready.id() };
                    PalwNativeReadyResponseV3::Signature(sign_with(&k, message.as_byte_slice(), PALW_MATERIAL_READY_CONTEXT_V3)?)
                }
            };
            let signer_key = if behaviour == Behaviour::WrongKey { Rc::new(key(200)) } else { k.clone() };
            let s = move |m: &[u8], c: &[u8]| sign_with(&signer_key, m, c);
            let s: &dyn Fn(&[u8], &[u8]) -> Result<Vec<u8>, String> = &s;
            provider_transport_response_v1(&request.header, 1_000, borsh::to_vec(&answer).unwrap(), vec![(provider.bond, s)])
        }
    }

    fn exchange(behaviours: &[Behaviour]) -> (ProviderExchangeV3, Vec<PalwBondKeyV2>, Rc<RefCell<Vec<(PalwBondKeyV2, &'static str)>>>, PalwMaterialManifestV1) {
        let executor = Rc::new(key(1));
        let root = Hash64::from_u64_word(42);
        let manifest = PalwMaterialManifestV1 { root, scopes: vec![test_scope()] };
        let seen = Rc::new(RefCell::new(Vec::new()));
        let mut keys = BTreeMap::new();
        let mut endpoints = Vec::new();
        let mut bonds = Vec::new();
        for (i, b) in behaviours.iter().enumerate() {
            let k = Rc::new(key(10 + i as u8));
            let id = bond(100 + i as u64);
            endpoints.push(ProviderEndpointV3 {
                bond: id,
                url: format!("http://127.0.0.1:{}", 9000 + i),
                pubkey: k.verification_key.as_ref().to_vec(),
                operator_id: Hash64::from_u64_word(500 + i as u64),
            });
            keys.insert(id, (k, *b));
            bonds.push(id);
        }
        let link = MockProviders { keys, manifest: manifest.clone(), seen: seen.clone() };
        let ex = executor.clone();
        let x = ProviderExchangeV3::new(
            Hash64::from_u64_word(7),
            bond(1),
            executor.verification_key.as_ref().to_vec(),
            endpoints,
            1 << 20,
            Box::new(link),
            Box::new(move |m, c| sign_with(&ex, m, c)),
            Box::new(|| 1_000),
        )
        .unwrap();
        (x, bonds, seen, manifest)
    }

    fn test_scope() -> kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialScopeV1 {
        let codec = kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialCodecV1::NativeHeldComplete;
        kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialScopeV1 {
            claim: Hash64::from_u64_word(5),
            commitment: Hash64::from_u64_word(6),
            codec,
            bytes: 64,
            units: 1,
            content: Hash64::from_u64_word(8),
        }
    }

    fn row_for(roster: &[PalwBondKeyV2], root: Hash64) -> PalwMonetaryMaterialRowV1 {
        let mut row: PalwMonetaryMaterialRowV1 = borsh::from_slice(&row_zero_bytes()).expect("a zeroed row decodes");
        row.terms.root = root;
        row.providers = roster
            .iter()
            .map(|b| {
                let mut p: kaspa_consensus_core::palw_monetary_material_v1::PalwMonetaryMaterialProviderV1 =
                    borsh::from_slice(&provider_zero_bytes()).expect("a zeroed provider decodes");
                p.provider = *b;
                p
            })
            .collect();
        row
    }
    fn row_zero_bytes() -> Vec<u8> {
        (0..65536).map(|len| vec![0u8; len]).find(|b| borsh::from_slice::<PalwMonetaryMaterialRowV1>(b).is_ok()).expect("zero row")
    }
    fn provider_zero_bytes() -> Vec<u8> {
        (0..4096)
            .map(|len| vec![0u8; len])
            .find(|b| borsh::from_slice::<kaspa_consensus_core::palw_monetary_material_v1::PalwMonetaryMaterialProviderV1>(b).is_ok())
            .expect("zero provider")
    }
    fn attempt() -> PalwAttemptUnsignedV2 {
        (0..65536).map(|len| vec![0u8; len]).find_map(|b| borsh::from_slice::<PalwAttemptUnsignedV2>(&b).ok()).expect("zero attempt")
    }
    fn ready(roster: &[PalwBondKeyV2], manifest: PalwMaterialManifestV1) -> PalwNativeMaterialReadyV3 {
        PalwNativeMaterialReadyV3 {
            version: 3,
            session: manifest.root,
            consent: Hash64::from_u64_word(3),
            binding: kaspa_consensus_core::palw_attempt_material_v3::PalwNativeMaterialBindingV3 {
                claim: Hash64::from_u64_word(5),
                execution_key: Hash64::from_u64_word(4),
                manifest,
            },
            providers: roster.to_vec(),
        }
    }

    #[test]
    fn native_exchange_fetches_one_agreed_manifest_from_every_roster_provider() {
        let (mut x, bonds, seen, manifest) = exchange(&[Behaviour::Honest, Behaviour::Honest]);
        let row = row_for(&bonds, manifest.root);
        let got = x.manifest(&row, &attempt(), b"capture").unwrap();
        assert_eq!(got, manifest);
        assert_eq!(seen.borrow().iter().filter(|(_, k)| *k == "manifest").count(), 2);
    }

    #[test]
    fn native_exchange_refuses_disagreeing_unknown_or_foreign_manifests() {
        let (mut x, bonds, _, manifest) = exchange(&[Behaviour::Honest, Behaviour::OtherManifest]);
        assert!(x.manifest(&row_for(&bonds, manifest.root), &attempt(), b"c").unwrap_err().contains("different manifests"));
        let (mut x, bonds, _, manifest) = exchange(&[Behaviour::Honest]);
        let mut roster = bonds.clone();
        roster.push(bond(999));
        assert!(x.manifest(&row_for(&roster, manifest.root), &attempt(), b"c").unwrap_err().contains("no configured endpoint"));
        let (mut x, bonds, _, _) = exchange(&[Behaviour::Honest]);
        assert!(x.manifest(&row_for(&bonds, Hash64::from_u64_word(1)), &attempt(), b"c").unwrap_err().contains("session's"));
    }

    #[test]
    fn native_exchange_refuses_a_response_not_signed_by_the_proven_key() {
        let (mut x, bonds, _, manifest) = exchange(&[Behaviour::WrongKey]);
        assert!(x.manifest(&row_for(&bonds, manifest.root), &attempt(), b"c").is_err());
    }

    #[test]
    fn native_exchange_collects_verified_ready_signatures_in_roster_order() {
        let (mut x, bonds, _, manifest) = exchange(&[Behaviour::Honest, Behaviour::Honest]);
        let r = ready(&bonds, manifest);
        let sigs = x.collect(&r, b"capture").unwrap();
        assert_eq!(sigs.keys().copied().collect::<Vec<_>>(), bonds);
        let (mut x, bonds, _, manifest) = exchange(&[Behaviour::Honest, Behaviour::BadReadySignature]);
        assert!(x.collect(&ready(&bonds, manifest), b"capture").unwrap_err().contains("does not verify"));
    }

    #[test]
    fn native_provider_flag_parses_and_refuses_malformed_text() {
        let txid = Hash64::from_u64_word(9).to_string();
        let f = parse_provider_flag_v3(&format!("{txid}:2=http://10.0.0.1:7000=0aff")).unwrap();
        assert_eq!(f.bond, PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(9), 2)));
        assert_eq!(f.url, "http://10.0.0.1:7000");
        assert_eq!(f.pubkey, ProviderKeySourceV3::Hex(vec![0x0a, 0xff]));
        assert_eq!(parse_provider_flag_v3(&format!("{txid}:0=https://p=@/k.hex")).unwrap().pubkey, ProviderKeySourceV3::File("/k.hex".into()));
        for bad in [format!("{txid}:0=http://p"), format!("{txid}=http://p=00"), format!("{txid}:0=ftp://p=00"), format!("{txid}:0=http://p=0g")] {
            assert!(parse_provider_flag_v3(&bad).is_err(), "{bad}");
        }
    }

    /// **audit(c) C-02 reproducer**: consensus re-selects the canonical session with the operator of EVERY bond on chain
    /// (`verify_prior` → `select_native_session_v3(.., |p| state.bond(p).operator_id)`), but the remote miner answers `operator_of`
    /// only for the providers it was configured with. A smaller-root session whose providers the miner does not know is healthy on
    /// chain and invisible here, so the miner mounts its Ready on a LATER session — which `verify_prior` refuses ("native ready
    /// substitutes prior session/provider roster") after the inference, the draw and the attempt signature.
    #[test]
    fn audit_c02_remote_miner_session_choice_diverges_from_consensus_with_partial_provider_config() {
        let (mut x, bonds, _, _) = exchange(&[Behaviour::Honest, Behaviour::Honest]);
        let a = attempt();
        let daa = 10;
        let session = kaspa_consensus_core::palw_attempt_material_v3::PalwNativeMaterialSessionV3 {
            version: 3,
            class_id: a.class_id,
            artifact_root: a.artifact_root,
            executor: PalwBondKeyV2(a.executor_bond),
            operator: a.operator_id,
            job_policy: kaspa_consensus_core::palw_attempt_material_v3::native_job_policy_v3(),
            codec: kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialCodecV1::NativeHeldComplete,
            max_units: 1,
            one_claim_cap: 1,
            admit_from_daa: 0,
            admit_until_daa: 100,
        };
        assert!(session.matches(&a, daa));
        // Row A: the canonical (smaller root) session, served by two providers this miner was NOT configured with.
        let unknown = [bond(900), bond(901)];
        let mut row_a = row_for(&unknown, Hash64::from_u64_word(1));
        row_a.terms.native_session = Some(session.clone());
        // Row B: a later session whose roster is exactly the configured providers.
        let mut row_b = row_for(&bonds, Hash64::from_u64_word(2));
        row_b.terms.native_session = Some(session);
        let rows = vec![row_a.clone(), row_b.clone()];
        // Consensus: every bond's operator is on chain.
        let chain_operator = |b: &PalwBondKeyV2| Some(b.0.transaction_id); // distinct per bond (verify-3: byte[0] of from_u64_word was 0 for every bond)
        let consensus = select_native_session_v3(rows.clone(), &a, daa, chain_operator).expect("consensus selects a session");
        assert_eq!(consensus.terms.root, row_a.terms.root, "consensus picks the canonical smaller-root session");
        // The remote miner over the same authenticated rows.
        x.set_rows(rows, daa);
        let miner = x.session(&a, daa);
        // verify(3): expected-vs-actual. The miner must either select consensus's canonical session (then hold at the manifest
        // round for lack of endpoints) or hold now; any OTHER session is refused by verify_prior only after inference, draw and
        // the journaled attempt signature.
        let miner_root = miner.as_ref().map(|r| r.terms.root);
        assert!(
            miner_root.is_none() || miner_root == Some(consensus.terms.root),
            "C-02: remote miner selected session {:?}, consensus verify_prior requires {:?} (row B={:?})",
            miner_root,
            consensus.terms.root,
            row_b.terms.root
        );
    }

    #[test]
    fn native_exchange_session_counts_only_proven_operators() {
        let (mut x, bonds, _, manifest) = exchange(&[Behaviour::Honest]);
        // One proven operator: never a healthy (>= 2 operators) session, whatever the rows say.
        let mut roster = bonds.clone();
        roster.push(bond(777));
        x.set_rows(vec![row_for(&roster, manifest.root)], 10);
        assert!(x.session(&attempt(), 10).is_none());
    }
}
