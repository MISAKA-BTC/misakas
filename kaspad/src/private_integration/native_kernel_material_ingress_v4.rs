//! Private explicit kernel ingress: actual producer-signed work, own native
//! validation, durable complete material, then provider 240/242 signatures.
//! A queued carrier still requires positive native admission before claim commit.
use super::*;
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_consensus_core::{
    palw_attempt_material_v3::verify_ready_signature_v3 as verify,
    palw_kernel_material_enrollment_v4::PalwKernelMaterialIngressOwnedV4 as Ingress,
    palw_material_release_recovery_v2::{PalwMaterialRecoveryActionV2 as Recovery, material_recovery_object_v2},
    palw_monetary_material_v1::PalwMonetaryMaterialActionV1 as Monetary,
    palw_native_resource_v1::{
        NativeHeapV1, NativeRetainedV1, NativeWireReaderV1, NativeWireScanV1, NativeWireShapeV1, preflight_native_value_v1,
        reserve_native_requests_v1, retain_native_value_decode_v1,
    },
    palw_prospective_kernel_material_v4::PalwProspectiveKernelMaterialRequestV4 as Material,
    palw_provider_court_v1::{PALW_PROVIDER_COURT_MLDSA87_CONTEXT_V1 as CONTEXT, palw_provider_court_message_v1 as message},
    palw_state_v2::PalwConsensusObjectV2 as Object,
};
use misaka_palw_remote::native_carrier_queue_v4::NativeCarrierPurposeV4;
const RESPONSE_CAP: u64 = 1 << 20;
#[derive(BorshSerialize, BorshDeserialize)]
struct Request {
    material: Material,
    object: Option<Object>,
}
impl NativeWireScanV1 for Request {
    fn scan_native_wire_v1<R: Read>(r: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        r.step()?;
        Material::scan_native_wire_v1(r)?.merge(Option::<Object>::scan_native_wire_v1(r)?)
    }
}
#[derive(BorshSerialize, BorshDeserialize)]
enum Response {
    Signed { request: Hash64, consent: Hash64, signatures: Vec<(PalwBondKeyV2, Vec<u8>, Vec<u8>)> },
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
            _ => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "kernel ingress response unknown")),
        }
    }
}
fn recovery_id(t: &kaspa_consensus_core::palw_material_release_recovery_v2::PalwMaterialRecoveryTermsV2) -> Hash64 {
    Hash64::from_bytes(misaka_palw_kernel::hash::object_id(b"MISAKA/PALW/RELEASE-RECOVERY-CONSENT/V2", t))
}
fn workspace(ingress: &Ingress) -> Result<Option<kaspa_consensus_core::palw_native_resource_v1::NativeDemandLeaseV1>, String> {
    let action = &*ingress.enrollment;
    let n = match action {
        Recovery::FundedEnroll { providers, .. } => providers.len() as u64,
        _ => return Err("kernel ingress proposal not atomic FundedEnroll".into()),
    };
    let wire = borsh::object_length(action).map_err(|e| e.to_string())? as u64;
    let heap = action.native_heap_v1("kernel signed proposal").bytes().ok_or("kernel proposal heap unsupported")?;
    let signatures = (3 * n + 2)
        .checked_mul(kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN as u64)
        .ok_or("kernel signature size overflow")?;
    let requests = wire
        .checked_mul(8)
        .and_then(|v| v.checked_add(heap))
        .and_then(|v| v.checked_add(signatures * 3))
        .and_then(|v| {
            v.checked_add(
                n * (std::mem::size_of::<(PalwBondKeyV2, Vec<u8>, Vec<u8>)>()
                    + 2 * std::mem::size_of::<(PalwBondKeyV2, Vec<u8>)>()
                    + std::mem::size_of::<ProviderTransportSignerV1<'_>>()) as u64,
            )
        })
        .ok_or("kernel ingress workspace overflow")?;
    reserve_native_requests_v1(requests).map_err(|e| e.to_string())
}
impl LoopbackNativeProviderV3 {
    fn kernel_request_v4(
        &self,
        request: &ProviderTransportRequestV1,
    ) -> Result<(Response, Option<kaspa_consensus_core::palw_native_resource_v1::NativeDemandLeaseV1>), String> {
        preflight_native_value_v1::<Request>(request.body.as_slice(), request.body.len() as u64).map_err(|e| e.to_string())?;
        let r =
            retain_native_value_decode_v1(&request.body, || borsh::from_slice::<Request>(&request.body)).map_err(|e| e.to_string())?;
        let api = self.consensus_manager.consensus().unguarded_session_blocking();
        let ingress = api.palw_kernel_material_ingress_v4(r.material.intent.checkpoint, &r.material)?;
        let intent = &r.material.intent;
        if request.header.domain != ingress.network
            || request.header.sender != intent.producer
            || request.header.sender_pubkey.as_slice() != ingress.verified.producer_pubkey()
            || request.header.recipients != intent.providers
            || request.header.daa_score != intent.commit_daa
        {
            return Err("kernel transport substitutes signed native producer/work/roster".into());
        }
        let lease = workspace(&ingress)?;
        let Recovery::FundedEnroll { monetary, terms, .. } = &*ingress.enrollment else {
            return Err("kernel proposal missing".into());
        };
        let mut served = 0;
        // Every named provider must actually have the registered key and a
        // durable bounded store. Partial responses cannot acquire a covenant.
        for p in &self.providers {
            if !intent.providers.contains(&p.bond) {
                continue;
            }
            if ingress.verified.provider_pubkey(&p.bond) != Some(p.key.public_key()) {
                return Err("kernel provider key differs from own native bond".into());
            }
            p.store.lock().map_err(|_| "kernel provider store poisoned")?.prepare_prospective_kernel_v4(
                p.bond,
                monetary,
                &ingress.verified,
                &r.material.bundle,
                &ingress.context,
                &ingress.enrollment,
            )?;
            served += 1;
        }
        if served != intent.providers.len() {
            return Err("kernel ingress endpoint must explicitly host complete roster".into());
        }
        if let Some(object) = &r.object {
            let Object::MonetaryMaterialActionV1 { action, signer, .. } = object else {
                return Err("kernel enrollment is not monetary carrier".into());
            };
            let Monetary::RecoveryV2(signed) = &**action else { return Err("kernel enrollment is not recovery carrier".into()) };
            if *signer != intent.producer || signed.payload() != ingress.enrollment.payload() {
                return Err("kernel enrollment substitutes own signed native proposal".into());
            }
            if !matches!(
                api.palw_object_rehearsal_v1(object),
                Some(kaspa_consensus_core::palw_producer_v2::PalwObjectRehearsalV1::Accepted)
            ) {
                return Err("native validator refuses kernel FundedEnroll".into());
            }
            self.additional_carriers.lock().map_err(|_| "kernel carrier queue poisoned")?.insert(
                monetary.terms.root,
                NativeCarrierPurposeV4::KernelEnrollmentV4 {
                    root: monetary.terms.root,
                    consent: monetary.id(),
                    recovery: recovery_id(terms),
                    until_daa: monetary.terms.enrollment_until_daa,
                },
                object,
            )?;
            return Ok((Response::Queued { root: monetary.terms.root, consent: monetary.id(), recovery: recovery_id(terms) }, lease));
        }
        let ordinary = monetary.payload();
        let recovery = ingress.enrollment.payload();
        let signatures: Vec<(PalwBondKeyV2, Vec<u8>, Vec<u8>)> = intent
            .providers
            .iter()
            .map(|bond| {
                let p = self.providers.iter().find(|p| p.bond == *bond).expect("complete configured roster checked");
                (
                    *bond,
                    p.key.sign_with_context(message(ingress.network, 240, bond, &ordinary).as_byte_slice(), CONTEXT).to_vec(),
                    p.key.sign_with_context(message(ingress.network, 242, bond, &recovery).as_byte_slice(), CONTEXT).to_vec(),
                )
            })
            .collect();
        let mut capsule = (*ingress.enrollment).clone();
        let consent = {
            let Recovery::FundedEnroll { monetary, providers, .. } = &mut capsule else { unreachable!() };
            for (offer, (bond, ordinary, _)) in monetary.offers.iter_mut().zip(&signatures) {
                if offer.provider != *bond {
                    return Err("kernel signed capsule roster differs".into());
                }
                offer.signature = ordinary.clone();
            }
            for (provider, (bond, _, recovery)) in providers.iter_mut().zip(&signatures) {
                if provider.provider != *bond {
                    return Err("kernel signed capsule recovery roster differs".into());
                }
                provider.signature = recovery.clone();
            }
            monetary.id()
        };
        // A crash cannot externally expose a signature whose full native
        // verification context/signature capsule is not durably recoverable.
        for p in self.providers.iter().filter(|p| intent.providers.contains(&p.bond)) {
            p.store.lock().map_err(|_| "kernel provider store poisoned")?.persist_kernel_signatures_v4(&ingress.context, &capsule)?;
        }
        Ok((Response::Signed { request: r.material.id(), consent, signatures }, lease))
    }
}
impl AuthenticatedSocketExchangeV3 {
    fn kernel_roundtrip_v4(
        &self,
        material: &Material,
        ingress: &Ingress,
        object: Option<&Object>,
    ) -> Result<NativeRetainedV1<Response>, String> {
        if self.sender != material.intent.producer
            || self.key.public_key() != ingress.verified.producer_pubkey()
            || self.recovery_roster != material.intent.providers
        {
            return Err("kernel ingress differs from explicit local producer/roster".into());
        }
        let length = borsh::object_length(&(material, object)).map_err(|e| e.to_string())? as u64;
        if length > PRIVATE_NATIVE_EXCHANGE_MAX_FRAME_V3 {
            return Err("kernel ingress exceeds actual transport frame cap".into());
        }
        let n = material.intent.providers.len() as u64;
        let _frames = reserve_native_requests_v1(
            length
                .checked_mul(4)
                .and_then(|v| {
                    v.checked_add(
                        n * kaspa_consensus_core::mldsa87_primitives::MLDSA87_PUBKEY_LEN as u64
                            + n * std::mem::size_of::<(PalwBondKeyV2, Vec<u8>)>() as u64
                            + RESPONSE_CAP * 3,
                    )
                })
                .ok_or("kernel transport size overflow")?,
        )
        .map_err(|e| e.to_string())?;
        let keys: Vec<_> = material
            .intent
            .providers
            .iter()
            .map(|b| Ok((*b, ingress.verified.provider_pubkey(b).ok_or("kernel native provider key absent")?.to_vec())))
            .collect::<Result<_, String>>()?;
        let body = borsh::to_vec(&(material, object)).map_err(|e| e.to_string())?;
        let signed = provider_transport_request_v1(
            ProviderTransportPurposeV1::KernelMaterialIngressV4,
            self.policy.domain,
            self.sender,
            self.key.public_key().to_vec(),
            material.intent.providers.clone(),
            request_nonce_v3(&body),
            now_unix_ms(),
            material.intent.commit_daa,
            body,
            |m, c| Ok(self.key.sign_with_context(m, c).to_vec()),
        )?;
        let mut socket = TcpStream::connect_timeout(&self.addr, Duration::from_secs(10)).map_err(|e| e.to_string())?;
        socket.set_read_timeout(Some(PRIVATE_NATIVE_EXCHANGE_TIMEOUT_V3)).map_err(|e| e.to_string())?;
        socket.set_write_timeout(Some(PRIVATE_NATIVE_EXCHANGE_TIMEOUT_V3)).map_err(|e| e.to_string())?;
        write_frame_v3(&mut socket, &borsh::to_vec(&signed).map_err(|e| e.to_string())?)?;
        let frame = read_frame_capped_v4(&mut socket, RESPONSE_CAP)?;
        preflight_native_value_v1::<ProviderTransportResponseV1>(&frame[..], frame.len() as u64).map_err(|e| e.to_string())?;
        let response = retain_native_value_decode_v1(&frame, || borsh::from_slice::<ProviderTransportResponseV1>(&frame))
            .map_err(|e| e.to_string())?;
        let responders = provider_transport_verify_response_v1(&response, &signed.header, &keys, &self.policy, now_unix_ms())?;
        if responders.iter().ne(material.intent.providers.iter()) {
            return Err("kernel ingress partial authenticated roster".into());
        }
        preflight_native_value_v1::<Response>(response.body.as_slice(), response.body.len() as u64).map_err(|e| e.to_string())?;
        retain_native_value_decode_v1(&response.body, || borsh::from_slice::<Response>(&response.body)).map_err(|e| e.to_string())
    }
    pub(super) fn ingest_kernel_v4(&self, material: &Material, ingress: &Ingress) -> Result<(), String> {
        if ingress.verified.request_id() != material.id() {
            return Err("kernel native grant belongs to another material request".into());
        }
        let _workspace = workspace(ingress)?;
        let Recovery::FundedEnroll { monetary, terms, .. } = &*ingress.enrollment else {
            return Err("kernel enrollment proposal absent".into());
        };
        let response = self.kernel_roundtrip_v4(material, ingress, None)?;
        let Response::Signed { request, consent, signatures } = &*response else {
            return Err("kernel provider did not sign durable complete material".into());
        };
        if *request != material.id()
            || *consent != monetary.id()
            || signatures.iter().map(|(b, _, _)| b).ne(material.intent.providers.iter())
        {
            return Err("kernel provider signed another request/consent/roster".into());
        }
        let ordinary = monetary.payload();
        let recovery = ingress.enrollment.payload();
        for (b, a, c) in signatures {
            let key = ingress.verified.provider_pubkey(b).ok_or("kernel native provider key absent")?;
            if !verify(key, message(ingress.network, 240, b, &ordinary).as_byte_slice(), a, CONTEXT)
                || !verify(key, message(ingress.network, 242, b, &recovery).as_byte_slice(), c, CONTEXT)
            {
                return Err("kernel provider liability signatures invalid".into());
            }
        }
        let object = material_recovery_object_v2(ingress.network, (*ingress.enrollment).clone(), self.sender, |b, m, c| {
            if *b == self.sender {
                return Some(self.key.sign_with_context(m, c).to_vec());
            }
            signatures.iter().find(|(p, _, _)| p == b).and_then(|(_, a, r)| {
                if m == message(ingress.network, 240, b, &ordinary).as_byte_slice() {
                    Some(a.clone())
                } else if m == message(ingress.network, 242, b, &recovery).as_byte_slice() {
                    Some(r.clone())
                } else {
                    None
                }
            })
        })
        .ok_or("kernel sponsor/provider FundedEnroll signature assembly failed")?;
        match &*self.kernel_roundtrip_v4(material, ingress, Some(&object))? {
            Response::Queued { root, consent, recovery }
                if *root == monetary.terms.root && *consent == monetary.id() && *recovery == recovery_id(terms) =>
            {
                Ok(())
            }
            Response::Refused(reason) => Err(reason.clone()),
            _ => Err("kernel provider did not durably queue exact atomic enrollment".into()),
        }
    }
}
pub(super) fn handle_kernel_v4(stream: &mut TcpStream, service: &LoopbackNativeProviderV3, request: &ProviderTransportRequestV1) {
    let local = service.consensus_manager.consensus().unguarded_session().get_virtual_daa_score();
    let Ok(mut replay) = service.replay.lock() else { return };
    if provider_transport_verify_request_v1(
        request,
        ProviderTransportPurposeV1::KernelMaterialIngressV4,
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
    let result = service.kernel_request_v4(request);
    let (body, _lease) = match result {
        Ok(value) => value,
        Err(reason) => {
            let Ok(lease) = reserve_native_requests_v1(RESPONSE_CAP * 4) else { return };
            if reason.len() as u64 > RESPONSE_CAP / 2 {
                return;
            }
            (Response::Refused(reason), lease)
        }
    };
    let n = request.header.recipients.len() as u64;
    let Ok(_response) = reserve_native_requests_v1(
        RESPONSE_CAP * 3
            + n * (std::mem::size_of::<ProviderTransportSignerV1<'_>>() as u64
                + kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN as u64),
    ) else {
        return;
    };
    let signers: Vec<_> = service
        .providers
        .iter()
        .filter(|p| request.header.recipients.contains(&p.bond))
        .map(|p| (p.bond, move |m: &[u8], c: &[u8]| -> Result<Vec<u8>, String> { Ok(p.key.sign_with_context(m, c).to_vec()) }))
        .collect();
    let responders: Vec<(PalwBondKeyV2, ProviderTransportSignerV1<'_>)> =
        signers.iter().map(|(b, s)| (*b, s as ProviderTransportSignerV1<'_>)).collect();
    if let Ok(response) = borsh::to_vec(&body)
        .map_err(|e| e.to_string())
        .and_then(|body| provider_transport_response_v1(&request.header, now_unix_ms(), body, responders))
        .and_then(|r| borsh::to_vec(&r).map_err(|e| e.to_string()))
    {
        if response.len() as u64 <= RESPONSE_CAP {
            let _ = write_frame_v3(stream, &response);
        }
    }
}

/// The roster is fixed for this service instance. Previously hosted identities
/// can leave obligations on disk; their owned metadata remains charged at the
/// next startup rather than silently granting each replacement roster a fresh
/// whole DB allowance. The node datadir is exclusively locked by its daemon.
pub(super) fn unhosted_metadata_reservation_v4(dir: &Path, keys: &[PrivateNativeProviderKeyV3]) -> Result<u64, String> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(e.to_string()),
    };
    let _workspace = reserve_native_requests_v1(
        (keys.len() as u64).checked_mul(256).and_then(|n| n.checked_add(8192)).ok_or("provider metadata roster inventory overflow")?,
    )
    .map_err(|e| e.to_string())?;
    let prefixes: Vec<_> = keys.iter().map(|p| format!("{}-{}.", p.bond.0.transaction_id, p.bond.0.index)).collect();
    let mut reserved = 0u64;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !(name.ends_with(".kernel-context-v4")
            || name.ends_with(".kernel-context-expiry-v4")
            || name.ends_with(".kernel-context-pending-v4")
            || name.ends_with(".expiry-pending-v4"))
        {
            continue;
        }
        if prefixes.iter().any(|p| name.starts_with(p)) {
            continue;
        }
        if !entry.file_type().map_err(|e| e.to_string())?.is_file() {
            return Err("unhosted kernel metadata is not an owned regular file".into());
        }
        let bytes = entry.metadata().map_err(|e| e.to_string())?.len();
        reserved = reserved
            .checked_add(
                bytes.checked_mul(2).and_then(|n| n.checked_add(272)).ok_or("unhosted metadata publication reservation overflow")?,
            )
            .ok_or("unhosted metadata total overflow")?;
    }
    Ok(reserved)
}
