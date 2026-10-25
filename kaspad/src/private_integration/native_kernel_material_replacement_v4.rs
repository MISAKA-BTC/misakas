//! Explicit bounded replacement of material providers. Original producer work
//! bytes stay immutable; fresh native 241/242 consent covers the new roster and
//! admission deadline. A signed/queued proposal grants no native acceptance.
use super::*;
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_consensus_core::{
    palw_attempt_material_v3::verify_ready_signature_v3 as verify,
    palw_kernel_material_context_v4::PalwProspectiveKernelMaterialContextV4 as Context,
    palw_kernel_material_enrollment_v4::kernel_material_capsule_identity_v4 as identity,
    palw_kernel_material_replacement_v4::{
        PalwKernelMaterialReplacementIntentV4 as Intent, PalwKernelMaterialReplacementOwnedV4 as Grant,
    },
    palw_material_release_recovery_v2::{PalwMaterialRecoveryActionV2 as Recovery, material_recovery_object_v2},
    palw_monetary_material_v1::PalwMonetaryMaterialActionV1 as Monetary,
    palw_native_resource_v1::{
        NativeDemandLeaseV1, NativeHeapV1, NativeRetainedV1, NativeWireReaderV1, NativeWireScanV1, NativeWireShapeV1,
        preflight_native_value_v1, reserve_native_requests_v1, retain_native_value_decode_v1,
    },
    palw_prospective_kernel_material_v4::PalwProspectiveKernelBundleV4 as Bundle,
    palw_provider_court_v1::{PALW_PROVIDER_COURT_MLDSA87_CONTEXT_V1 as DOMAIN, palw_provider_court_message_v1 as message},
    palw_state_v2::PalwConsensusObjectV2 as Object,
};
use misaka_palw_remote::native_carrier_queue_v4::{NativeCarrierPurposeV4, kernel_replacement_carrier_id_v4};
const RESPONSE_CAP: u64 = 1 << 20;
#[derive(BorshSerialize, BorshDeserialize)]
struct Request {
    context: Context,
    bundle: Bundle,
    intent: Intent,
    object: Option<Object>,
}
impl NativeWireScanV1 for Request {
    fn scan_native_wire_v1<R: Read>(r: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        r.step()?;
        Context::scan_native_wire_v1(r)?
            .merge(Bundle::scan_native_wire_v1(r)?)?
            .merge(Intent::scan_native_wire_v1(r)?)?
            .merge(Option::<Object>::scan_native_wire_v1(r)?)
    }
}
#[derive(BorshSerialize, BorshDeserialize)]
enum Response {
    Signed { consent: Hash64, recovery: Hash64, signatures: Vec<(PalwBondKeyV2, Vec<u8>, Vec<u8>)> },
    Queued { root: Hash64, consent: Hash64, recovery: Hash64 },
    Refused(String),
}
impl NativeWireScanV1 for Response {
    fn scan_native_wire_v1<R: Read>(r: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        r.step()?;
        match r.byte()? {
            0 => <(Hash64, Hash64, Vec<(PalwBondKeyV2, Vec<u8>, Vec<u8>)>)>::scan_native_wire_v1(r),
            1 => <(Hash64, Hash64, Hash64)>::scan_native_wire_v1(r),
            2 => String::scan_native_wire_v1(r),
            _ => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "replacement response variant unknown")),
        }
    }
}
fn workspace(grant: &Grant) -> Result<Option<NativeDemandLeaseV1>, String> {
    let action = &*grant.proposal;
    let Recovery::ReplaceBeforeV4 { providers, .. } = action else { return Err("replacement grant kind differs".into()) };
    let n = providers.len() as u64;
    let wire = borsh::object_length(action).map_err(|e| e.to_string())? as u64;
    let heap = action.native_heap_v1("replacement signed capsule").bytes().ok_or("replacement draft heap unknown")?;
    let signatures = (3 * n + 2)
        .checked_mul(kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN as u64)
        .ok_or("replacement signature extent overflow")?;
    let requests = wire
        .checked_mul(8)
        .and_then(|v| v.checked_add(heap))
        .and_then(|v| v.checked_add(signatures * 3))
        .and_then(|v| {
            v.checked_add(
                n * (std::mem::size_of::<(PalwBondKeyV2, Vec<u8>, Vec<u8>)>() + 2 * std::mem::size_of::<(PalwBondKeyV2, Vec<u8>)>())
                    as u64,
            )
        })
        .ok_or("replacement workspace overflow")?;
    reserve_native_requests_v1(requests).map_err(|e| e.to_string())
}
impl LoopbackNativeProviderV3 {
    fn replacement_request_v4(&self, request: &ProviderTransportRequestV1) -> Result<(Response, Option<NativeDemandLeaseV1>), String> {
        preflight_native_value_v1::<Request>(request.body.as_slice(), request.body.len() as u64).map_err(|e| e.to_string())?;
        let r =
            retain_native_value_decode_v1(&request.body, || borsh::from_slice::<Request>(&request.body)).map_err(|e| e.to_string())?;
        let api = self.consensus_manager.consensus().unguarded_session_blocking();
        let grant = api.palw_kernel_material_replacement_v4(&r.context, &r.bundle, &r.intent)?;
        let (e, consent, _) = identity(&grant.proposal)?;
        let Recovery::ReplaceBeforeV4 { recovery, replacement, .. } = &*grant.proposal else {
            return Err("replacement own proposal absent".into());
        };
        if request.header.domain != grant.network
            || request.header.sender != e.terms.sponsor
            || request.header.sender_pubkey.as_slice() != grant.producer_pubkey()
            || request.header.recipients != r.intent.providers
        {
            return Err("replacement transport substitutes own sponsor/native keys/roster".into());
        }
        let lease = workspace(&grant)?;
        let mut served = 0;
        for p in self.providers.iter().filter(|p| r.intent.providers.contains(&p.bond)) {
            if grant.provider_pubkey(&p.bond) != Some(p.key.public_key()) {
                return Err("replacement provider key differs from native bond".into());
            }
            p.store
                .lock()
                .map_err(|_| "replacement store poisoned")?
                .prepare_replacement_kernel_v4(p.bond, &grant, &r.bundle, &r.context)?;
            served += 1;
        }
        if served != r.intent.providers.len() {
            return Err("replacement endpoint must explicitly host entire new roster".into());
        }
        if let Some(object) = &r.object {
            let Object::MonetaryMaterialActionV1 { action, signer, .. } = object else {
                return Err("replacement carrier kind differs".into());
            };
            let Monetary::RecoveryV2(signed) = &**action else { return Err("replacement recovery carrier absent".into()) };
            if *signer != e.terms.sponsor || signed.payload() != grant.proposal.payload() {
                return Err("replacement carrier substitutes own native proposal".into());
            }
            if !matches!(
                api.palw_object_rehearsal_v1(object),
                Some(kaspa_consensus_core::palw_producer_v2::PalwObjectRehearsalV1::Accepted)
            ) {
                return Err("native admission refuses signed bounded replacement".into());
            }
            self.additional_carriers.lock().map_err(|_| "replacement carrier queue poisoned")?.insert(
                kernel_replacement_carrier_id_v4(e.terms.root),
                NativeCarrierPurposeV4::KernelReplacementV4 {
                    root: e.terms.root,
                    consent,
                    recovery: *recovery,
                    until_daa: r.intent.valid_until_daa,
                },
                object,
            )?;
            return Ok((Response::Queued { root: e.terms.root, consent, recovery: *recovery }, lease));
        }
        let ordinary = replacement.payload();
        let recovery_payload = grant.proposal.payload();
        let signatures: Vec<_> = r
            .intent
            .providers
            .iter()
            .map(|bond| {
                let p = self.providers.iter().find(|p| p.bond == *bond).expect("complete configured roster checked");
                (
                    *bond,
                    p.key.sign_with_context(message(grant.network, 241, bond, &ordinary).as_byte_slice(), DOMAIN).to_vec(),
                    p.key.sign_with_context(message(grant.network, 242, bond, &recovery_payload).as_byte_slice(), DOMAIN).to_vec(),
                )
            })
            .collect();
        let mut capsule = (*grant.proposal).clone();
        let Recovery::ReplaceBeforeV4 { replacement, providers, .. } = &mut capsule else { unreachable!() };
        for (offer, (bond, signature, _)) in replacement.enrollment.offers.iter_mut().zip(&signatures) {
            if offer.provider != *bond {
                return Err("replacement offer order differs".into());
            }
            offer.signature = signature.clone();
        }
        for (provider, (bond, _, signature)) in providers.iter_mut().zip(&signatures) {
            if provider.provider != *bond {
                return Err("replacement recovery order differs".into());
            }
            provider.signature = signature.clone();
        }
        // Publication of all raw data and the new signed capsule precedes any
        // external liability signature. Cold reopen uses the original work and
        // this new generation; it never rewrites old provider consent.
        for p in self.providers.iter().filter(|p| r.intent.providers.contains(&p.bond)) {
            p.store.lock().map_err(|_| "replacement store poisoned")?.persist_kernel_signatures_v4(&r.context, &capsule)?;
        }
        Ok((Response::Signed { consent, recovery: *recovery, signatures }, lease))
    }
}
impl AuthenticatedSocketExchangeV3 {
    fn replacement_roundtrip_v4(
        &self,
        context: &Context,
        bundle: &Bundle,
        intent: &Intent,
        grant: &Grant,
        object: Option<&Object>,
    ) -> Result<NativeRetainedV1<Response>, String> {
        let (e, _, _) = identity(&grant.proposal)?;
        if self.sender != e.terms.sponsor || self.key.public_key() != grant.producer_pubkey() {
            return Err("replacement sender differs from explicit native sponsor".into());
        }
        let length = borsh::object_length(&(context, bundle, intent, object)).map_err(|e| e.to_string())? as u64;
        if length > PRIVATE_NATIVE_EXCHANGE_MAX_FRAME_V3 {
            return Err("replacement material/context exceeds transport frame cap".into());
        }
        let n = intent.providers.len() as u64;
        let _frames = reserve_native_requests_v1(
            length
                .checked_mul(4)
                .and_then(|v| {
                    v.checked_add(
                        RESPONSE_CAP * 3
                            + n * (kaspa_consensus_core::mldsa87_primitives::MLDSA87_PUBKEY_LEN
                                + std::mem::size_of::<(PalwBondKeyV2, Vec<u8>)>()) as u64,
                    )
                })
                .ok_or("replacement transport extent overflow")?,
        )
        .map_err(|e| e.to_string())?;
        let keys: Vec<_> = intent
            .providers
            .iter()
            .map(|b| Ok((*b, grant.provider_pubkey(b).ok_or("replacement native provider key absent")?.to_vec())))
            .collect::<Result<_, String>>()?;
        let body = borsh::to_vec(&(context, bundle, intent, object)).map_err(|e| e.to_string())?;
        let signed = provider_transport_request_v1(
            ProviderTransportPurposeV1::KernelMaterialReplacementV4,
            self.policy.domain,
            self.sender,
            self.key.public_key().to_vec(),
            intent.providers.clone(),
            request_nonce_v3(&body),
            now_unix_ms(),
            grant.daa_score,
            body,
            |m, c| Ok(self.key.sign_with_context(m, c).to_vec()),
        )?;
        let mut socket = TcpStream::connect_timeout(&self.addr, Duration::from_secs(10)).map_err(|e| e.to_string())?;
        socket.set_read_timeout(Some(PRIVATE_NATIVE_EXCHANGE_TIMEOUT_V3)).map_err(|e| e.to_string())?;
        socket.set_write_timeout(Some(PRIVATE_NATIVE_EXCHANGE_TIMEOUT_V3)).map_err(|e| e.to_string())?;
        write_frame_v3(&mut socket, &borsh::to_vec(&signed).map_err(|e| e.to_string())?)?;
        let raw = read_frame_capped_v4(&mut socket, RESPONSE_CAP)?;
        preflight_native_value_v1::<ProviderTransportResponseV1>(&raw[..], raw.len() as u64).map_err(|e| e.to_string())?;
        let response = retain_native_value_decode_v1(&raw, || borsh::from_slice::<ProviderTransportResponseV1>(&raw))
            .map_err(|e| e.to_string())?;
        let responders = provider_transport_verify_response_v1(&response, &signed.header, &keys, &self.policy, now_unix_ms())?;
        if responders.iter().ne(intent.providers.iter()) {
            return Err("replacement response omits/substitutes native providers".into());
        }
        preflight_native_value_v1::<Response>(response.body.as_slice(), response.body.len() as u64).map_err(|e| e.to_string())?;
        retain_native_value_decode_v1(&response.body, || borsh::from_slice::<Response>(&response.body)).map_err(|e| e.to_string())
    }
    pub(super) fn replace_kernel_v4(&self, context: &Context, bundle: &Bundle, intent: &Intent, grant: &Grant) -> Result<(), String> {
        let _workspace = workspace(grant)?;
        let (e, expected_consent, _) = identity(&grant.proposal)?;
        let Recovery::ReplaceBeforeV4 { replacement, recovery: expected_recovery, .. } = &*grant.proposal else {
            return Err("replacement own native grant absent".into());
        };
        let response = self.replacement_roundtrip_v4(context, bundle, intent, grant, None)?;
        let Response::Signed { consent, recovery, signatures } = &*response else {
            return Err("replacement provider did not sign durable material".into());
        };
        if *consent != expected_consent
            || recovery != expected_recovery
            || signatures.iter().map(|(b, _, _)| b).ne(intent.providers.iter())
        {
            return Err("replacement signatures differ from own exact generation".into());
        }
        let ordinary = replacement.payload();
        let recovery_payload = grant.proposal.payload();
        for (bond, a, b) in signatures {
            let key = grant.provider_pubkey(bond).ok_or("replacement native provider key absent")?;
            if !verify(key, message(grant.network, 241, bond, &ordinary).as_byte_slice(), a, DOMAIN)
                || !verify(key, message(grant.network, 242, bond, &recovery_payload).as_byte_slice(), b, DOMAIN)
            {
                return Err("replacement liability signatures invalid".into());
            }
        }
        let object = material_recovery_object_v2(grant.network, (*grant.proposal).clone(), self.sender, |bond, m, c| {
            if *bond == self.sender {
                return Some(self.key.sign_with_context(m, c).to_vec());
            }
            signatures.iter().find(|(p, _, _)| p == bond).and_then(|(_, a, b)| {
                if m == message(grant.network, 241, bond, &ordinary).as_byte_slice() {
                    Some(a.clone())
                } else if m == message(grant.network, 242, bond, &recovery_payload).as_byte_slice() {
                    Some(b.clone())
                } else {
                    None
                }
            })
        })
        .ok_or("replacement sponsor/provider signature assembly failed")?;
        match &*self.replacement_roundtrip_v4(context, bundle, intent, grant, Some(&object))? {
            Response::Queued { root, consent, recovery }
                if *root == e.terms.root && *consent == expected_consent && recovery == expected_recovery =>
            {
                Ok(())
            }
            Response::Refused(reason) => Err(reason.clone()),
            _ => Err("exact bounded replacement not durably queued".into()),
        }
    }
}
pub(super) fn handle_replacement_v4(stream: &mut TcpStream, service: &LoopbackNativeProviderV3, request: &ProviderTransportRequestV1) {
    let local = service.consensus_manager.consensus().unguarded_session().get_virtual_daa_score();
    let Ok(mut replay) = service.replay.lock() else { return };
    if provider_transport_verify_request_v1(
        request,
        ProviderTransportPurposeV1::KernelMaterialReplacementV4,
        &service.transport,
        now_unix_ms(),
        Some(local),
        &mut replay,
    )
    .is_err()
    {
        return;
    }
    drop(replay);
    let result = service.replacement_request_v4(request);
    let (body, _lease) = match result {
        Ok(v) => v,
        Err(reason) => {
            if reason.len() as u64 > RESPONSE_CAP / 2 {
                return;
            }
            let Ok(lease) = reserve_native_requests_v1(RESPONSE_CAP * 4) else { return };
            (Response::Refused(reason), lease)
        }
    };
    let Ok(_framing) = reserve_native_requests_v1(RESPONSE_CAP * 4) else { return };
    let signers: Vec<_> = service
        .providers
        .iter()
        .filter(|p| request.header.recipients.contains(&p.bond))
        .map(|p| (p.bond, move |m: &[u8], c: &[u8]| -> Result<Vec<u8>, String> { Ok(p.key.sign_with_context(m, c).to_vec()) }))
        .collect();
    let responders: Vec<(PalwBondKeyV2, ProviderTransportSignerV1<'_>)> =
        signers.iter().map(|(b, s)| (*b, s as ProviderTransportSignerV1<'_>)).collect();
    let response = borsh::to_vec(&body)
        .map_err(|e| e.to_string())
        .and_then(|body| provider_transport_response_v1(&request.header, now_unix_ms(), body, responders))
        .and_then(|r| borsh::to_vec(&r).map_err(|e| e.to_string()));
    if let Ok(bytes) = response {
        if bytes.len() as u64 <= RESPONSE_CAP {
            let _ = write_frame_v3(stream, &bytes);
        }
    }
}
