//! **RFC-0008 PAV3 provider transport v1: an integrity-authenticated envelope for producer↔provider exchanges between hosts**
//! (2026-10-15, round 3; unverified source — nothing here has been compiled or run).
//!
//! The PAV3 exchange carries public material only: the candidate header, the signed base attempt, the Ready draft, the
//! capture and the prompt are all bytes a provider later publishes or the chain already holds. Confidentiality is therefore
//! NOT a goal and this protocol is TLS-free: anyone on the path may read every frame. What it guarantees is integrity and
//! mutual authentication bound to the parties' REGISTERED ML-DSA-87 bond keys:
//!
//! * **Request.** The sender (the executor) signs `ProviderTransportRequestHeaderV1::id` under
//!   [`PALW_PROVIDER_TRANSPORT_REQUEST_CONTEXT_V1`]. The header binds the chain/network domain, the purpose, the sender bond
//!   and its public key, the exact recipient provider bonds, a 32-byte nonce, the wall-clock issue time, the DAA score the
//!   request is about, and the length and digest of the body. A provider verifies the signature BEFORE any expensive work,
//!   then (outside this module) binds the key to the chain: the body's signed attempt must name the same bond and public key,
//!   and the provider's own Consensus API authority read refuses an attempt whose key is not the bond's registered key.
//! * **Response.** Every answering provider bond signs `ProviderTransportResponseHeaderV1::id` under
//!   [`PALW_PROVIDER_TRANSPORT_RESPONSE_CONTEXT_V1`]; the header binds the request id, the responders, the issue time and the
//!   body digest. The client verifies each signature with the roster keys ITS OWN Consensus API returned — never with a key the
//!   response names — and refuses a responder outside the request's recipients.
//! * **Replay.** A provider admits each `(sender, nonce)` once inside the skew window ([`ProviderTransportReplayGuardV1`]),
//!   only after the signature verified (unauthenticated bytes cannot fill it), and refuses — rather than evicts a live entry —
//!   when full. Outside the window the timestamp check refuses it. The DAA window refuses a request about a height the
//!   provider's node is not near; an unknown local DAA refuses.
//! * **Bounds.** Body length is declared, signed and capped; frames are the bounded length-prefixed frames of the loopback
//!   transport; timeouts are the caller's socket timeouts.
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_consensus_core::{Hash64, palw_attempt_material_v3::verify_ready_signature_v3, palw_state_v2::PalwBondKeyV2};
use std::collections::{BTreeMap, BTreeSet};

pub const PALW_PROVIDER_TRANSPORT_VERSION_V1: u16 = 1;
pub const PALW_PROVIDER_TRANSPORT_REQUEST_CONTEXT_V1: &[u8] = b"MISAKA/PALW/PROVIDER-TRANSPORT/REQUEST/V1";
pub const PALW_PROVIDER_TRANSPORT_RESPONSE_CONTEXT_V1: &[u8] = b"MISAKA/PALW/PROVIDER-TRANSPORT/RESPONSE/V1";
/// Accepted wall-clock difference between the two hosts, either direction.
pub const PALW_PROVIDER_TRANSPORT_MAX_SKEW_MS_V1: u64 = 120_000;
/// Accepted distance between the request's DAA score and the provider node's virtual DAA, either direction.
pub const PALW_PROVIDER_TRANSPORT_DAA_WINDOW_V1: u64 = 600;
/// Live `(sender, nonce)` entries a provider keeps; past it new requests are refused until entries age out.
pub const PALW_PROVIDER_TRANSPORT_REPLAY_CAPACITY_V1: usize = 65_536;
/// At most the PAV3 roster size.
pub const PALW_PROVIDER_TRANSPORT_MAX_PARTIES_V1: usize =
    kaspa_consensus_core::palw_attempt_material_v3::PALW_MATERIAL_MAX_PROVIDERS_V3;
const MLDSA87_PUBKEY_LEN: usize = kaspa_consensus_core::mldsa87_primitives::MLDSA87_PUBKEY_LEN;

/// What the body is. One purpose today; a new one is a new variant, never a reinterpretation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum ProviderTransportPurposeV1 {
    /// Body: borsh `PalwNativeReadyRequestV3` (request) / `PalwNativeReadyResponseV3` (response).
    NativeReadyV3,
    /// Independent V4 canonical prospective enrollment, never reinterprets Ready V3.
    RecoveryEnrollmentV4,
    /// Producer-signed actual kernel work and full typed material, followed by
    /// atomic 240/242 FundedEnroll. Old native Ready/enrollment are unchanged.
    KernelMaterialIngressV4,
    /// Read one already-retained complete kernel unit; no new credit or liability.
    KernelMaterialQueryV4,
    /// Sponsor-signed bounded replacement of a kernel root's provider roster (variant 4): the original producer work and
    /// context are replayed by every new provider, which persists material and the new signed generation before
    /// answering 241/242. A queued replacement is not native acceptance.
    KernelMaterialReplacementV4,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ProviderTransportRequestHeaderV1 {
    pub version: u16,
    pub purpose: ProviderTransportPurposeV1,
    /// `palw_network_domain_v2_for(net, genesis)`: the chain the request is about.
    pub domain: Hash64,
    pub sender: PalwBondKeyV2,
    pub sender_pubkey: Vec<u8>,
    /// The provider bonds asked, strictly ascending.
    pub recipients: Vec<PalwBondKeyV2>,
    pub nonce: [u8; 32],
    pub issued_unix_ms: u64,
    pub daa_score: u64,
    pub body_len: u64,
    pub body_digest: Hash64,
}
impl ProviderTransportRequestHeaderV1 {
    pub fn id(&self) -> Hash64 {
        Hash64::from_bytes(misaka_palw_kernel::hash::object_id(b"MISAKA/PALW/PROVIDER-TRANSPORT/REQUEST-ID/V1", self))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ProviderTransportRequestV1 {
    pub header: ProviderTransportRequestHeaderV1,
    pub signature: Vec<u8>,
    pub body: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ProviderTransportResponseHeaderV1 {
    pub version: u16,
    pub purpose: ProviderTransportPurposeV1,
    pub domain: Hash64,
    pub request_id: Hash64,
    /// The provider bonds that sign this response, strictly ascending, a subset of the request's recipients.
    pub responders: Vec<PalwBondKeyV2>,
    pub issued_unix_ms: u64,
    pub body_len: u64,
    pub body_digest: Hash64,
}
impl ProviderTransportResponseHeaderV1 {
    pub fn id(&self) -> Hash64 {
        Hash64::from_bytes(misaka_palw_kernel::hash::object_id(b"MISAKA/PALW/PROVIDER-TRANSPORT/RESPONSE-ID/V1", self))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ProviderTransportResponseV1 {
    pub header: ProviderTransportResponseHeaderV1,
    /// One per responder, in responder order.
    pub signatures: Vec<Vec<u8>>,
    pub body: Vec<u8>,
}

pub fn provider_transport_body_digest_v1(purpose: ProviderTransportPurposeV1, body: &[u8]) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(b"MISAKA/PALW/PROVIDER-TRANSPORT/BODY/V1").to_state();
    state.update(&borsh::to_vec(&purpose).expect("purpose serializes"));
    crate::put_len(&mut state, body);
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// What one side accepts. `domain` is the chain this host runs; `max_body` caps the declared body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProviderTransportPolicyV1 {
    pub domain: Hash64,
    pub max_skew_ms: u64,
    pub daa_window: u64,
    pub max_body: u64,
}
impl ProviderTransportPolicyV1 {
    pub fn new(domain: Hash64, max_body: u64) -> Self {
        Self {
            domain,
            max_skew_ms: PALW_PROVIDER_TRANSPORT_MAX_SKEW_MS_V1,
            daa_window: PALW_PROVIDER_TRANSPORT_DAA_WINDOW_V1,
            max_body,
        }
    }
    fn fresh(&self, issued: u64, now: u64) -> bool {
        issued.abs_diff(now) <= self.max_skew_ms
    }
}

fn strictly_ascending(bonds: &[PalwBondKeyV2]) -> bool {
    !bonds.is_empty() && bonds.len() <= PALW_PROVIDER_TRANSPORT_MAX_PARTIES_V1 && bonds.windows(2).all(|pair| pair[0] < pair[1])
}

/// **The sender's half**: a signed request over `body`. `sign(message, context)` is the sender bond's ML-DSA-87 key.
#[allow(clippy::too_many_arguments)]
pub fn provider_transport_request_v1(
    purpose: ProviderTransportPurposeV1,
    domain: Hash64,
    sender: PalwBondKeyV2,
    sender_pubkey: Vec<u8>,
    mut recipients: Vec<PalwBondKeyV2>,
    nonce: [u8; 32],
    issued_unix_ms: u64,
    daa_score: u64,
    body: Vec<u8>,
    sign: impl FnOnce(&[u8], &[u8]) -> Result<Vec<u8>, String>,
) -> Result<ProviderTransportRequestV1, String> {
    recipients.sort();
    recipients.dedup();
    if !strictly_ascending(&recipients) || sender_pubkey.len() != MLDSA87_PUBKEY_LEN || nonce == [0u8; 32] {
        return Err("provider transport request needs 1..=8 recipients, an ML-DSA-87 sender key and a nonzero nonce".into());
    }
    let header = ProviderTransportRequestHeaderV1 {
        version: PALW_PROVIDER_TRANSPORT_VERSION_V1,
        purpose,
        domain,
        sender,
        sender_pubkey,
        recipients,
        nonce,
        issued_unix_ms,
        daa_score,
        body_len: body.len() as u64,
        body_digest: provider_transport_body_digest_v1(purpose, &body),
    };
    let signature = sign(header.id().as_byte_slice(), PALW_PROVIDER_TRANSPORT_REQUEST_CONTEXT_V1)?;
    Ok(ProviderTransportRequestV1 { header, signature, body })
}

/// **Replay protection on the provider.** Keys `(sender, nonce)` with their issue time; entries older than twice the skew
/// window are pruned (the timestamp check already refuses them). Full: refuse, never evict a live entry.
#[derive(Debug)]
pub struct ProviderTransportReplayGuardV1 {
    capacity: usize,
    window_ms: u64,
    seen: BTreeMap<(PalwBondKeyV2, [u8; 32]), u64>,
    by_time: BTreeSet<(u64, PalwBondKeyV2, [u8; 32])>,
}
impl ProviderTransportReplayGuardV1 {
    pub fn new(capacity: usize, max_skew_ms: u64) -> Self {
        Self { capacity: capacity.max(1), window_ms: max_skew_ms.saturating_mul(2), seen: BTreeMap::new(), by_time: BTreeSet::new() }
    }
    pub fn len(&self) -> usize {
        self.seen.len()
    }
    pub fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }
    fn prune(&mut self, now: u64) {
        while let Some(&(issued, sender, nonce)) = self.by_time.first() {
            if issued.saturating_add(self.window_ms) >= now {
                break;
            }
            self.by_time.pop_first();
            self.seen.remove(&(sender, nonce));
        }
    }
    pub fn admit(&mut self, sender: PalwBondKeyV2, nonce: [u8; 32], issued: u64, now: u64) -> Result<(), String> {
        self.prune(now);
        if self.seen.contains_key(&(sender, nonce)) {
            return Err("provider transport request replays a (sender, nonce) already admitted".into());
        }
        if self.seen.len() >= self.capacity {
            return Err("provider transport replay window is full; retry later".into());
        }
        self.seen.insert((sender, nonce), issued);
        self.by_time.insert((issued, sender, nonce));
        Ok(())
    }
}

/// **The provider's half of request authentication.** Shape, domain, purpose, body digest and cap, freshness, DAA window,
/// the sender's signature, and only then the replay guard. `local_daa: None` (the node cannot say where it is) refuses.
/// Returns the request id. Binding `sender_pubkey` to the chain's registered key is the caller's (see the module comment).
pub fn provider_transport_verify_request_v1(
    request: &ProviderTransportRequestV1,
    purpose: ProviderTransportPurposeV1,
    policy: &ProviderTransportPolicyV1,
    now_ms: u64,
    local_daa: Option<u64>,
    guard: &mut ProviderTransportReplayGuardV1,
) -> Result<Hash64, String> {
    let h = &request.header;
    if h.version != PALW_PROVIDER_TRANSPORT_VERSION_V1 || h.purpose != purpose {
        return Err("provider transport request has an unknown version or another purpose".into());
    }
    if h.domain != policy.domain {
        return Err("provider transport request is for another chain/network domain".into());
    }
    if !strictly_ascending(&h.recipients) || h.sender_pubkey.len() != MLDSA87_PUBKEY_LEN || h.nonce == [0u8; 32] {
        return Err("provider transport request names malformed recipients, sender key or nonce".into());
    }
    if h.body_len != request.body.len() as u64 || h.body_len > policy.max_body {
        return Err("provider transport request body length differs from its signed length or exceeds the cap".into());
    }
    if provider_transport_body_digest_v1(purpose, &request.body) != h.body_digest {
        return Err("provider transport request body differs from its signed digest".into());
    }
    if !policy.fresh(h.issued_unix_ms, now_ms) {
        return Err("provider transport request is outside the accepted clock window".into());
    }
    let local = local_daa.ok_or("the provider node cannot report its DAA score; request refused")?;
    if h.daa_score.abs_diff(local) > policy.daa_window {
        return Err(format!("provider transport request DAA {} is outside {}±{}", h.daa_score, local, policy.daa_window));
    }
    let id = h.id();
    if !verify_ready_signature_v3(&h.sender_pubkey, id.as_byte_slice(), &request.signature, PALW_PROVIDER_TRANSPORT_REQUEST_CONTEXT_V1)
    {
        return Err("provider transport request signature does not verify under the sender key".into());
    }
    guard.admit(h.sender, h.nonce, h.issued_unix_ms, now_ms)?;
    Ok(id)
}

/// One responder's ML-DSA-87 signing function, `(message, context)`.
pub type ProviderTransportSignerV1<'a> = &'a dyn Fn(&[u8], &[u8]) -> Result<Vec<u8>, String>;

/// **The provider's response**: every `(bond, sign)` in `responders` signs the same header; they must be among the request's
/// recipients.
pub fn provider_transport_response_v1(
    request: &ProviderTransportRequestHeaderV1,
    issued_unix_ms: u64,
    body: Vec<u8>,
    mut responders: Vec<(PalwBondKeyV2, ProviderTransportSignerV1<'_>)>,
) -> Result<ProviderTransportResponseV1, String> {
    responders.sort_by_key(|(bond, _)| *bond);
    let bonds: Vec<PalwBondKeyV2> = responders.iter().map(|(bond, _)| *bond).collect();
    if !strictly_ascending(&bonds) || !bonds.iter().all(|bond| request.recipients.contains(bond)) {
        return Err("provider transport responders must be distinct recipients of the request".into());
    }
    let header = ProviderTransportResponseHeaderV1 {
        version: PALW_PROVIDER_TRANSPORT_VERSION_V1,
        purpose: request.purpose,
        domain: request.domain,
        request_id: request.id(),
        responders: bonds,
        issued_unix_ms,
        body_len: body.len() as u64,
        body_digest: provider_transport_body_digest_v1(request.purpose, &body),
    };
    let id = header.id();
    let signatures = responders
        .iter()
        .map(|(_, sign)| sign(id.as_byte_slice(), PALW_PROVIDER_TRANSPORT_RESPONSE_CONTEXT_V1))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ProviderTransportResponseV1 { header, signatures, body })
}

/// **The client's half of response authentication.** `roster_keys` are the registered keys the client's OWN chain read
/// returned for the recipients; a responder without one, outside the recipients, or with a non-verifying signature refuses
/// the whole response. Returns the responders.
pub fn provider_transport_verify_response_v1(
    response: &ProviderTransportResponseV1,
    request: &ProviderTransportRequestHeaderV1,
    roster_keys: &[(PalwBondKeyV2, Vec<u8>)],
    policy: &ProviderTransportPolicyV1,
    now_ms: u64,
) -> Result<Vec<PalwBondKeyV2>, String> {
    let h = &response.header;
    if h.version != PALW_PROVIDER_TRANSPORT_VERSION_V1
        || h.purpose != request.purpose
        || h.domain != request.domain
        || h.domain != policy.domain
        || h.request_id != request.id()
    {
        return Err("provider transport response answers another request, purpose or chain".into());
    }
    if !strictly_ascending(&h.responders)
        || !h.responders.iter().all(|bond| request.recipients.contains(bond))
        || response.signatures.len() != h.responders.len()
    {
        return Err("provider transport response names responders outside the request or unsigned".into());
    }
    if h.body_len != response.body.len() as u64
        || h.body_len > policy.max_body
        || provider_transport_body_digest_v1(h.purpose, &response.body) != h.body_digest
    {
        return Err("provider transport response body differs from its signed length/digest or exceeds the cap".into());
    }
    if !policy.fresh(h.issued_unix_ms, now_ms) {
        return Err("provider transport response is outside the accepted clock window".into());
    }
    let id = h.id();
    for (bond, signature) in h.responders.iter().zip(&response.signatures) {
        let key = roster_keys
            .iter()
            .find(|(roster, _)| roster == bond)
            .map(|(_, key)| key)
            .ok_or("provider transport responder has no registered roster key")?;
        if !verify_ready_signature_v3(key, id.as_byte_slice(), signature, PALW_PROVIDER_TRANSPORT_RESPONSE_CONTEXT_V1) {
            return Err(format!(
                "provider transport response signature of {}:{} does not verify under its registered key",
                bond.0.transaction_id, bond.0.index
            ));
        }
    }
    Ok(h.responders.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::tx::TransactionOutpoint;

    fn bond(word: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(word), 0))
    }
    fn key(seed: u8) -> libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair {
        libcrux_ml_dsa::ml_dsa_87::generate_key_pair([seed; 32])
    }
    fn signer(k: &libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair) -> impl Fn(&[u8], &[u8]) -> Result<Vec<u8>, String> + '_ {
        move |m: &[u8], c: &[u8]| {
            libcrux_ml_dsa::ml_dsa_87::sign(&k.signing_key, m, c, [7u8; 32]).map(|s| s.as_ref().to_vec()).map_err(|e| format!("{e:?}"))
        }
    }
    const NOW: u64 = 1_800_000_000_000;

    fn request(executor: &libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair, nonce: u8) -> ProviderTransportRequestV1 {
        provider_transport_request_v1(
            ProviderTransportPurposeV1::NativeReadyV3,
            Hash64::from_u64_word(1),
            bond(90),
            executor.verification_key.as_ref().to_vec(),
            vec![bond(5), bond(4)],
            [nonce; 32],
            NOW,
            1_000,
            b"ready request".to_vec(),
            signer(executor),
        )
        .unwrap()
    }

    #[test]
    fn appended_kernel_query_preserves_previous_purpose_bytes() {
        for (value, tag) in [
            (ProviderTransportPurposeV1::NativeReadyV3, 0u8),
            (ProviderTransportPurposeV1::RecoveryEnrollmentV4, 1),
            (ProviderTransportPurposeV1::KernelMaterialIngressV4, 2),
            (ProviderTransportPurposeV1::KernelMaterialQueryV4, 3),
            (ProviderTransportPurposeV1::KernelMaterialReplacementV4, 4),
        ] {
            assert_eq!(borsh::to_vec(&value).unwrap(), [tag]);
            assert_eq!(borsh::from_slice::<ProviderTransportPurposeV1>(&[tag]).unwrap(), value);
        }
        assert!(borsh::from_slice::<ProviderTransportPurposeV1>(&[5]).is_err());
    }
    #[test]
    fn a_signed_request_verifies_once_and_every_tamper_or_replay_is_refused() {
        let executor = key(1);
        let policy = ProviderTransportPolicyV1::new(Hash64::from_u64_word(1), 1 << 20);
        let mut guard = ProviderTransportReplayGuardV1::new(16, policy.max_skew_ms);
        let purpose = ProviderTransportPurposeV1::NativeReadyV3;
        let good = request(&executor, 3);
        assert_eq!(good.header.recipients, vec![bond(4), bond(5)], "recipients are canonicalized");
        provider_transport_verify_request_v1(&good, purpose, &policy, NOW, Some(1_100), &mut guard).unwrap();
        assert!(provider_transport_verify_request_v1(&good, purpose, &policy, NOW, Some(1_100), &mut guard).is_err(), "replay");
        let fresh = || ProviderTransportReplayGuardV1::new(16, policy.max_skew_ms);
        let mut body = request(&executor, 4);
        body.body[0] ^= 1;
        assert!(provider_transport_verify_request_v1(&body, purpose, &policy, NOW, Some(1_000), &mut fresh()).is_err());
        let mut domain = request(&executor, 4);
        domain.header.domain = Hash64::from_u64_word(2);
        assert!(provider_transport_verify_request_v1(&domain, purpose, &policy, NOW, Some(1_000), &mut fresh()).is_err());
        let late = request(&executor, 4);
        assert!(
            provider_transport_verify_request_v1(&late, purpose, &policy, NOW + policy.max_skew_ms + 1, Some(1_000), &mut fresh())
                .is_err()
        );
        assert!(provider_transport_verify_request_v1(&late, purpose, &policy, NOW, Some(1_000 + 601), &mut fresh()).is_err());
        assert!(provider_transport_verify_request_v1(&late, purpose, &policy, NOW, None, &mut fresh()).is_err(), "unknown DAA");
        let mut other_key = request(&executor, 4);
        other_key.header.sender_pubkey = key(2).verification_key.as_ref().to_vec();
        assert!(provider_transport_verify_request_v1(&other_key, purpose, &policy, NOW, Some(1_000), &mut fresh()).is_err());
        let small = ProviderTransportPolicyV1 { max_body: 3, ..policy };
        assert!(provider_transport_verify_request_v1(&late, purpose, &small, NOW, Some(1_000), &mut fresh()).is_err());
    }

    #[test]
    fn the_replay_guard_refuses_when_full_and_forgets_only_expired_entries() {
        let mut guard = ProviderTransportReplayGuardV1::new(2, 10);
        guard.admit(bond(1), [1; 32], 100, 100).unwrap();
        guard.admit(bond(1), [2; 32], 100, 100).unwrap();
        assert!(guard.admit(bond(1), [3; 32], 100, 100).is_err(), "full: refuse, never evict a live entry");
        guard.admit(bond(1), [3; 32], 125, 125).unwrap();
        assert_eq!(guard.len(), 1, "entries older than twice the skew are pruned");
    }

    #[test]
    fn a_response_verifies_only_under_the_clients_roster_keys() {
        let executor = key(1);
        let (a, b) = (key(4), key(5));
        let policy = ProviderTransportPolicyV1::new(Hash64::from_u64_word(1), 1 << 20);
        let req = request(&executor, 9);
        let (sa, sb) = (signer(&a), signer(&b));
        let (sa, sb): (ProviderTransportSignerV1<'_>, ProviderTransportSignerV1<'_>) = (&sa, &sb);
        let response =
            provider_transport_response_v1(&req.header, NOW, b"signed".to_vec(), vec![(bond(5), sb), (bond(4), sa)]).unwrap();
        let roster = vec![(bond(4), a.verification_key.as_ref().to_vec()), (bond(5), b.verification_key.as_ref().to_vec())];
        assert_eq!(
            provider_transport_verify_response_v1(&response, &req.header, &roster, &policy, NOW).unwrap(),
            vec![bond(4), bond(5)]
        );
        let swapped = vec![(bond(4), b.verification_key.as_ref().to_vec()), (bond(5), a.verification_key.as_ref().to_vec())];
        assert!(provider_transport_verify_response_v1(&response, &req.header, &swapped, &policy, NOW).is_err());
        let mut tampered = response.clone();
        tampered.body[0] ^= 1;
        assert!(provider_transport_verify_response_v1(&tampered, &req.header, &roster, &policy, NOW).is_err());
        let other = request(&executor, 10);
        assert!(provider_transport_verify_response_v1(&response, &other.header, &roster, &policy, NOW).is_err(), "another request");
        assert!(provider_transport_response_v1(&req.header, NOW, vec![], vec![(bond(6), sa)]).is_err(), "not a recipient");
    }
}

// The transport's owner supplies its exact external codec to the native
// allocation scanner; semantic authentication still follows this preflight.
impl kaspa_consensus_core::palw_native_resource_v1::NativeWireScanV1 for ProviderTransportPurposeV1 {
    fn scan_native_wire_v1<R: std::io::Read>(
        r: &mut kaspa_consensus_core::palw_native_resource_v1::NativeWireReaderV1<R>,
    ) -> std::io::Result<kaspa_consensus_core::palw_native_resource_v1::NativeWireShapeV1> {
        r.step()?;
        match r.byte()? {
            0..=4 => Ok(Default::default()),
            _ => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "unknown provider transport purpose")),
        }
    }
}
macro_rules! transport_wire_v1 {($ty:ty;$($field:ty),+)=>{
    impl kaspa_consensus_core::palw_native_resource_v1::NativeWireScanV1 for $ty {
        fn scan_native_wire_v1<R:std::io::Read>(r:&mut kaspa_consensus_core::palw_native_resource_v1::NativeWireReaderV1<R>)->std::io::Result<kaspa_consensus_core::palw_native_resource_v1::NativeWireShapeV1>{
            use kaspa_consensus_core::palw_native_resource_v1::{NativeWireShapeV1,NativeWireScanV1};
            r.step()?;let shape=NativeWireShapeV1::default();$(let shape=shape.merge(<$field>::scan_native_wire_v1(r)?)?;)+Ok(shape)
        }
    }
};}
transport_wire_v1!(ProviderTransportRequestHeaderV1;u16,ProviderTransportPurposeV1,Hash64,PalwBondKeyV2,Vec<u8>,Vec<PalwBondKeyV2>,[u8;32],u64,u64,u64,Hash64);
transport_wire_v1!(ProviderTransportRequestV1;ProviderTransportRequestHeaderV1,Vec<u8>,Vec<u8>);
transport_wire_v1!(ProviderTransportResponseHeaderV1;u16,ProviderTransportPurposeV1,Hash64,Hash64,Vec<PalwBondKeyV2>,u64,u64,Hash64);
transport_wire_v1!(ProviderTransportResponseV1;ProviderTransportResponseHeaderV1,Vec<Vec<u8>>,Vec<u8>);
