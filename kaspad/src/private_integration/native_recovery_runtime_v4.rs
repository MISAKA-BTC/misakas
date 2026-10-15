//! Explicit V4 prospective provider/sponsor enrollment. The V3 Ready wire and
//! execution binding remain unchanged. A quote is authenticated prior state;
//! only native acceptance permits the producer to execute its next attempt.
use super::*;
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_consensus_core::{
    palw_attempt_material_v3::verify_ready_signature_v3 as verify,
    palw_material_release_recovery_v2::{PalwMaterialRecoveryActionV2 as Recovery, material_recovery_object_v2},
    palw_material_session_enrollment_v4::PalwNativeRecoverySessionQuoteV4 as Quote,
    palw_native_resource_v1::{
        NativeHeapV1, NativeWireReaderV1, NativeWireScanV1, NativeWireShapeV1, preflight_native_value_v1, reserve_native_requests_v1,
        retain_native_value_decode_v1,
    },
    palw_provider_court_v1::{PALW_PROVIDER_COURT_MLDSA87_CONTEXT_V1 as CONTEXT, palw_provider_court_message_v1 as message},
    palw_state_v2::PalwConsensusObjectV2 as Object,
};
use misaka_palw_remote::native_carrier_queue_v4::NativeCarrierPurposeV4;
const ENROLLMENT_FRAME_MAX: u64 = 1 << 20;
#[derive(BorshSerialize, BorshDeserialize)]
struct Request {
    quote: Quote,
    object: Option<Object>,
}
impl NativeWireScanV1 for Request {
    fn scan_native_wire_v1<R: Read>(r: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        r.step()?;
        Quote::scan_native_wire_v1(r)?.merge(Option::<Object>::scan_native_wire_v1(r)?)
    }
}
#[derive(BorshSerialize, BorshDeserialize)]
enum Response {
    Signed(Vec<(PalwBondKeyV2, Vec<u8>, Vec<u8>)>),
    Queued(Hash64),
    Refused(String),
}
impl NativeWireScanV1 for Response {
    fn scan_native_wire_v1<R: Read>(r: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        r.step()?;
        match r.byte()? {
            0 => Vec::<(PalwBondKeyV2, Vec<u8>, Vec<u8>)>::scan_native_wire_v1(r),
            1 => Hash64::scan_native_wire_v1(r),
            2 => String::scan_native_wire_v1(r),
            _ => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "unknown recovery response")),
        }
    }
}
fn recovery_id(terms: &kaspa_consensus_core::palw_material_release_recovery_v2::PalwMaterialRecoveryTermsV2) -> Hash64 {
    Hash64::from_bytes(misaka_palw_kernel::hash::object_id(b"MISAKA/PALW/RELEASE-RECOVERY-CONSENT/V2", terms))
}
fn ordinary_payload(q: &Quote) -> Result<Vec<u8>, String> {
    match q.proposal.as_ref() {
        Some(Recovery::FundedEnroll { monetary, .. }) => Ok(monetary.payload()),
        _ => Err("V4 quote has no fresh funded proposal".into()),
    }
}
struct ServerResponseOwnerV4 {
    body: Response,
    _lease: Option<kaspa_consensus_core::palw_native_resource_v1::NativeDemandLeaseV1>,
}
fn checked_sum(values: &[u64]) -> Result<u64, String> {
    values.iter().try_fold(0u64, |s, n| s.checked_add(*n)).ok_or("native enrollment workspace sum overflow".into())
}
fn sizes_v4(q: &Quote) -> Result<(u64, u64, u64), String> {
    let Some(Recovery::FundedEnroll { monetary, terms, .. }) = q.proposal.as_ref() else {
        return Err("V4 quote lacks funded terms".into());
    };
    let n = q.provider_keys.len() as u64;
    if !(2..=8).contains(&n) {
        return Err("V4 enrollment provider count outside signed roster bound".into());
    }
    let ordinary = checked_sum(&[
        borsh::object_length(&monetary.terms).map_err(|e| e.to_string())? as u64,
        4,
        n.checked_mul(68 + 8 + 8).ok_or("offer wire overflow")?,
    ])?;
    let recovery = checked_sum(&[
        4,
        b"material-recovery-funded-enroll/v2".len() as u64,
        4,
        ordinary,
        borsh::object_length(terms).map_err(|e| e.to_string())? as u64,
        4,
        n.checked_mul(68).ok_or("provider list overflow")?,
    ])?;
    let signatures = (n * 2 + 2)
        .checked_mul(kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN as u64)
        .ok_or("signed enrollment wire overflow")?;
    let signed_object =
        checked_sum(&[borsh::object_length(q.proposal.as_ref().unwrap()).map_err(|e| e.to_string())? as u64, signatures, 2, 68, 4])?;
    if signed_object > ENROLLMENT_FRAME_MAX {
        return Err("V4 signed enrollment exceeds canonical carrier cap".into());
    }
    Ok((ordinary, recovery, signed_object))
}
fn server_workspace_v4(q: &Quote) -> Result<Option<kaspa_consensus_core::palw_native_resource_v1::NativeDemandLeaseV1>, String> {
    let n = q.provider_keys.len() as u64;
    let (ordinary, recovery, _) = sizes_v4(q)?;
    let ml = kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN as u64;
    let body = 5 + n * (68 + 8 + 2 * ml); // Signed Vec of two actual ML-DSA signatures per provider.
    let header = 2 + 1 + 64 + 64 + 4 + n * 68 + 8 + 8 + 64;
    let outer = checked_sum(&[header, 4, n * (4 + ml), 4, body])?;
    let typed = n
        * (std::mem::size_of::<(PalwBondKeyV2, Vec<u8>, Vec<u8>)>()
            + std::mem::size_of::<&ServedProviderV3>()
            + std::mem::size_of::<(PalwBondKeyV2, usize)>()
            + std::mem::size_of::<(PalwBondKeyV2, ProviderTransportSignerV1<'_>)>()
            + 2 * std::mem::size_of::<PalwBondKeyV2>()
            + std::mem::size_of::<Vec<u8>>()) as u64;
    let requests = checked_sum(&[
        typed,
        3 * n * ml,
        body,
        outer,
        2 * ordinary,
        2 * recovery,
        n * (std::mem::size_of::<(PalwBondKeyV2, u64, u64)>() + std::mem::size_of::<PalwBondKeyV2>()) as u64,
    ])?;
    reserve_native_requests_v1(requests).map_err(|e| e.to_string())
}
impl LoopbackNativeProviderV3 {
    fn recovery_request_v4(&self, request: &ProviderTransportRequestV1) -> Result<ServerResponseOwnerV4, String> {
        if request.body.len() as u64 > ENROLLMENT_FRAME_MAX {
            return Err("V4 enrollment body exceeds exact local cap".into());
        }
        preflight_native_value_v1::<Request>(&request.body[..], request.body.len() as u64).map_err(|e| e.to_string())?;
        let r =
            retain_native_value_decode_v1(&request.body, || borsh::from_slice::<Request>(&request.body)).map_err(|e| e.to_string())?;
        let q = &r.quote;
        let roster: Vec<_> = q.provider_keys.iter().map(|(b, _)| *b).collect();
        let expected = self.consensus_manager.consensus().unguarded_session_blocking().palw_native_recovery_session_quote_v4(
            Some(q.checkpoint),
            q.daa,
            q.class,
            q.executor,
            roster.clone(),
        )?;
        if &expected != q
            || q.version != 4
            || q.network != self.transport.domain
            || q.executor != request.header.sender
            || q.executor_pubkey != request.header.sender_pubkey
            || request.header.daa_score != q.daa
            || request.header.recipients != roster
        {
            return Err("V4 enrollment substitutes native quote/executor/roster/prior-state".into());
        }
        let workspace = server_workspace_v4(q)?;
        let action = q.proposal.as_ref().ok_or("native quote already has an active session")?;
        let Recovery::FundedEnroll { monetary, terms, .. } = action else {
            return Err("V4 request is not atomic FundedEnroll".into());
        };
        // Refuse a class this actual provider cannot serve before it signs the
        // monetary obligation, rather than treating an admission signature as DA.
        let session = self.consensus_manager.consensus().unguarded_session_blocking();
        let artifact = monetary.terms.native_session.as_ref().ok_or("native session absent")?.artifact_root;
        let registry = crate::palw_backends::PalwBackendRegistry::for_node_v1(
            &self.config.params,
            self.court,
            self.config.params.palw_prompt_ids_form_v1(),
            self.holdings.clone(),
            self.config.params.net.to_string().into_bytes(),
            self.chain_classes,
        );
        let _backend = registry.resolve_or_chain(q.class, artifact, |id| session.palw_registered_class_carriage_v1(id))?;
        let served: Vec<_> = self.providers.iter().filter(|p| roster.contains(&p.bond)).collect();
        if served.len() != roster.len() {
            return Err("this V4 endpoint must explicitly serve the complete enrolled roster".into());
        }
        for p in &served {
            if !q.provider_keys.iter().any(|(b, key)| *b == p.bond && key.as_slice() == p.key.public_key()) {
                return Err("provider key differs from native registered bond".into());
            }
        }
        if let Some(object) = &r.object {
            // Every provider and the sponsor must authorize the SAME canonical
            // unsigned proposal; exact signatures are then validated natively.
            let Object::MonetaryMaterialActionV1 { action: signed, signer, .. } = object else {
                return Err("not a monetary enrollment carrier".into());
            };
            let kaspa_consensus_core::palw_monetary_material_v1::PalwMonetaryMaterialActionV1::RecoveryV2(signed) = &**signed else {
                return Err("not recovery carrier".into());
            };
            if signer != &q.executor || signed.payload() != action.payload() {
                return Err("signed enrollment substitutes quote".into());
            }
            if !matches!(
                session.palw_object_rehearsal_v1(object),
                Some(kaspa_consensus_core::palw_producer_v2::PalwObjectRehearsalV1::Accepted)
            ) {
                return Err("native acceptance rehearsal refuses funded enrollment".into());
            }
            let root = monetary.terms.root;
            let mut queue = self.additional_carriers.lock().map_err(|_| "additional queue poisoned")?;
            if let Some(id) = queue.pending_enrollment(q.class, q.executor) {
                return Ok(ServerResponseOwnerV4 { body: Response::Queued(id), _lease: workspace });
            }
            queue.insert(
                root,
                NativeCarrierPurposeV4::Enrollment {
                    class: q.class,
                    executor: q.executor,
                    root,
                    consent: monetary.id(),
                    recovery: recovery_id(terms),
                    until_daa: monetary.terms.enrollment_until_daa,
                },
                object,
            )?;
            kaspa_core::info!("RFC08 V4 atomic enrollment durably queued: class {} root {} consent {}", q.class, root, monetary.id());
            Ok(ServerResponseOwnerV4 { body: Response::Queued(root), _lease: workspace })
        } else {
            let ordinary = ordinary_payload(q)?;
            let recovery = action.payload();
            let signatures = served
                .into_iter()
                .map(|p| {
                    let a = message(q.network, 240, &p.bond, &ordinary);
                    let b = message(q.network, 242, &p.bond, &recovery);
                    (
                        p.bond,
                        p.key.sign_with_context(a.as_byte_slice(), CONTEXT).to_vec(),
                        p.key.sign_with_context(b.as_byte_slice(), CONTEXT).to_vec(),
                    )
                })
                .collect();
            Ok(ServerResponseOwnerV4 { body: Response::Signed(signatures), _lease: workspace })
        }
    }
}
impl AuthenticatedSocketExchangeV3 {
    fn recovery_roundtrip_v4(
        &self,
        q: &Quote,
        object: Option<Object>,
    ) -> Result<kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1<Response>, String> {
        if q.executor != self.sender || q.executor_pubkey.as_slice() != self.key.public_key() {
            return Err("local producer key/bond differs from recovery sponsor".into());
        }
        let frame_length = borsh::object_length(&(q, &object)).map_err(|e| e.to_string())? as u64;
        if frame_length > ENROLLMENT_FRAME_MAX {
            return Err("canonical enrollment request exceeds V4 cap before allocation".into());
        }
        let quote_heap = q.native_heap_v1("V4 transport quote copy").bytes().ok_or("V4 quote heap unsupported")?;
        let transport_fixed = 2 * (self.key.public_key().len() as u64)
            + q.provider_keys.len() as u64 * (2 * std::mem::size_of::<PalwBondKeyV2>()) as u64
            + 2 * kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN as u64;
        let _request_workspace = reserve_native_requests_v1(checked_sum(&[
            quote_heap,
            4 * frame_length,
            transport_fixed,
            std::mem::size_of::<Request>() as u64,
            std::mem::size_of::<ProviderTransportRequestV1>() as u64,
        ])?)
        .map_err(|e| e.to_string())?;
        let request = Request { quote: q.clone(), object };
        let body = borsh::to_vec(&request).map_err(|e| e.to_string())?;
        if body.len() as u64 > ENROLLMENT_FRAME_MAX {
            return Err("enrollment body exceeds capped transport".into());
        }
        let signed = provider_transport_request_v1(
            ProviderTransportPurposeV1::RecoveryEnrollmentV4,
            self.policy.domain,
            self.sender,
            self.key.public_key().to_vec(),
            q.provider_keys.iter().map(|(b, _)| *b).collect(),
            request_nonce_v3(&body),
            now_unix_ms(),
            q.daa,
            body,
            |m, c| Ok(self.key.sign_with_context(m, c).to_vec()),
        )?;
        let mut socket = TcpStream::connect_timeout(&self.addr, Duration::from_secs(10)).map_err(|e| e.to_string())?;
        socket.set_read_timeout(Some(PRIVATE_NATIVE_EXCHANGE_TIMEOUT_V3)).map_err(|e| e.to_string())?;
        socket.set_write_timeout(Some(PRIVATE_NATIVE_EXCHANGE_TIMEOUT_V3)).map_err(|e| e.to_string())?;
        write_frame_v3(&mut socket, &borsh::to_vec(&signed).map_err(|e| e.to_string())?)?;
        let frame = read_frame_capped_v4(&mut socket, ENROLLMENT_FRAME_MAX)?;
        preflight_native_value_v1::<ProviderTransportResponseV1>(&frame[..], frame.len() as u64).map_err(|e| e.to_string())?;
        let response = retain_native_value_decode_v1(&frame, || borsh::from_slice::<ProviderTransportResponseV1>(&frame))
            .map_err(|e| e.to_string())?;
        let responders =
            provider_transport_verify_response_v1(&response, &signed.header, &q.provider_keys, &self.policy, now_unix_ms())?;
        if !responders.iter().eq(q.provider_keys.iter().map(|(b, _)| b)) {
            return Err("incomplete authenticated recovery roster response".into());
        }
        preflight_native_value_v1::<Response>(&response.body[..], response.body.len() as u64).map_err(|e| e.to_string())?;
        retain_native_value_decode_v1(&response.body, || borsh::from_slice::<Response>(&response.body)).map_err(|e| e.to_string())
    }
    pub(super) fn enroll_recovery_session_v4(&self, q: &Quote) -> Result<(), String> {
        if self.recovery_roster.iter().copied().ne(q.provider_keys.iter().map(|(b, _)| *b)) {
            return Err("producer quote differs from explicit configured roster".into());
        }
        // Exact encoded proposal and all signature buffers coexist with the
        // retained provider response until the completed carrier is durable.
        let (ordinary_size, recovery_size, signed_wire) = sizes_v4(q)?;
        let proposal = q.proposal.as_ref().ok_or("missing funded proposal")?;
        let draft_heap =
            proposal.native_heap_v1("signed V4 enrollment clone").bytes().ok_or("signed V4 enrollment heap unsupported")?;
        let n = q.provider_keys.len() as u64;
        let signature_bytes = (2 * n + 2)
            .checked_mul(kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN as u64)
            .ok_or("signature construction overflow")?;
        let fixed = std::mem::size_of::<Object>()
            + std::mem::size_of::<kaspa_consensus_core::palw_monetary_material_v1::PalwMonetaryMaterialActionV1>()
            + std::mem::size_of::<Recovery>();
        let _workspace = reserve_native_requests_v1(checked_sum(&[
            draft_heap,
            signature_bytes,
            3 * signed_wire,
            2 * ordinary_size,
            2 * recovery_size,
            n * (std::mem::size_of::<(PalwBondKeyV2, u64, u64)>() + std::mem::size_of::<PalwBondKeyV2>()) as u64,
            fixed as u64,
        ])?)
        .map_err(|e| e.to_string())?;
        let response = self.recovery_roundtrip_v4(q, None)?;
        let Response::Signed(signatures) = &*response else { return Err("provider did not sign prospective enrollment".into()) };
        if !signatures.iter().map(|(b, _, _)| b).eq(q.provider_keys.iter().map(|(b, _)| b)) {
            return Err("provider returned reordered/partial recovery signatures".into());
        }
        let ordinary = ordinary_payload(q)?;
        let action = q.proposal.as_ref().ok_or("missing funded proposal")?;
        let recovery = action.payload();
        for ((bond, a, b), (_, key)) in signatures.iter().zip(&q.provider_keys) {
            if !verify(key, message(q.network, 240, bond, &ordinary).as_byte_slice(), a, CONTEXT)
                || !verify(key, message(q.network, 242, bond, &recovery).as_byte_slice(), b, CONTEXT)
            {
                return Err("provider recovery signature does not bind canonical native terms".into());
            }
        }
        let object = material_recovery_object_v2(q.network, action.clone(), q.executor, |bond, msg, context| {
            if *bond == self.sender {
                return Some(self.key.sign_with_context(msg, context).to_vec());
            }
            signatures.iter().find(|(b, _, _)| b == bond).and_then(|(_, a, b)| {
                if msg == message(q.network, 240, bond, &ordinary).as_byte_slice() {
                    Some(a.clone())
                } else if msg == message(q.network, 242, bond, &recovery).as_byte_slice() {
                    Some(b.clone())
                } else {
                    None
                }
            })
        })
        .ok_or("complete sponsor/provider enrollment signing failed")?;
        match &*self.recovery_roundtrip_v4(q, Some(object))? {
            Response::Queued(_) => Ok(()),
            Response::Refused(reason) => Err(reason.clone()),
            _ => Err("provider did not durably queue funded enrollment".into()),
        }
    }
}
pub(super) fn handle_recovery_v4(stream: &mut TcpStream, service: &LoopbackNativeProviderV3, request: &ProviderTransportRequestV1) {
    if request.body.len() as u64 > ENROLLMENT_FRAME_MAX {
        return;
    }
    let local = service.consensus_manager.consensus().unguarded_session().get_virtual_daa_score();
    let Ok(mut replay) = service.replay.lock() else { return };
    if provider_transport_verify_request_v1(
        request,
        ProviderTransportPurposeV1::RecoveryEnrollmentV4,
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
    let result = service.recovery_request_v4(request);
    // Errors after authentication may still be signed only by the exact
    // configured recipients. No absent provider is synthesized.
    let (body, _response_lease) = match result {
        Ok(owner) => (owner.body, owner._lease),
        Err(reason) => {
            let lease =
                reserve_native_requests_v1((reason.len() as u64).checked_add(5).and_then(|n| n.checked_mul(4)).unwrap_or(u64::MAX));
            let Ok(lease) = lease else { return };
            (Response::Refused(reason), lease)
        }
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
        let _ = write_frame_v3(stream, &response);
    }
}

impl LoopbackNativeProviderV3 {
    /// One outstanding authenticated event per provider across BOTH ACK and
    /// answer queues. Lock order matches GC: store -> ACK outbox -> additional.
    /// The durable provider store supplies actual typed court material.
    pub(super) fn answer_and_release_tick_v4(&self) -> Result<(), String> {
        if self.config.params.palw_material_release_recovery_v2.is_none() {
            return Ok(());
        }
        use kaspa_consensus_core::{
            palw_material_capacity_v1::PalwMonetaryMaterialDemandV1 as Demand,
            palw_material_release_recovery_v2::PalwMaterialRecoveryAnswerV4,
        };
        let api = self.consensus_manager.consensus().unguarded_session_blocking();
        let rows = api.palw_monetary_material_covenants_v1()?;
        let daa = api.get_virtual_daa_score();
        for provider in &self.providers {
            let store = provider.store.lock().map_err(|_| "provider answer store poisoned")?;
            let ack = provider.recovery_acks.lock().map_err(|_| "provider ACK queue poisoned")?;
            let mut additional = self.additional_carriers.lock().map_err(|_| "provider additional queue poisoned")?;
            if ack.pending().next().is_some() || additional.provider_in_flight(provider.bond) {
                continue;
            }
            let Some((_, checkpoint)) = api.palw_material_provider_ack_checkpoint_finalized_v2(provider.bond)? else { continue };
            'contracts: for row in &rows {
                for c in row.challenges.iter().filter(|c| c.provider == provider.bond && !c.answered && daa <= c.deadline_daa) {
                    let demand = if let Some(m) = c.native_missing {
                        Demand::Held(m)
                    } else if let Some(part) = c.segment_part {
                        Demand::Segmented { position: c.position, part }
                    } else {
                        Demand::Kernel { stage: c.stage, position: c.position }
                    };
                    let action = match store.answer_material_current_v4(
                        row.consent,
                        row.terms.root,
                        c.dependency,
                        provider.bond,
                        provider.key.public_key(),
                        demand,
                        &*api,
                    ) {
                        Ok(a) => a,
                        Err(error) => {
                            kaspa_core::warn!("RFC08 V4 actual court material unavailable: {error}");
                            continue;
                        }
                    };
                    let len = borsh::object_length(&*action).map_err(|e| e.to_string())? as u64;
                    let _workspace = reserve_native_requests_v1(
                        len.checked_mul(8)
                            .and_then(|n| n.checked_add(4 * kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN as u64))
                            .ok_or("answer construction workspace overflow")?,
                    )
                    .map_err(|e| e.to_string())?;
                    let (action, _answer_lease) = action.into_parts_v1();
                    let answer = PalwMaterialRecoveryAnswerV4::from_ordinary(action).map_err(str::to_owned)?;
                    let sequence = checkpoint.sequence.checked_add(1).ok_or("provider event sequence exhausted")?;
                    let action = Recovery::AnswerAndCheckpoint {
                        answer: Box::new(answer),
                        stream: checkpoint.stream,
                        previous: checkpoint.last_receipt,
                        sequence,
                        challenge_deadline_daa: c.deadline_daa,
                    };
                    let event = Hash64::from_bytes(misaka_palw_kernel::hash::object_id(
                        b"MISAKA/PALW/MATERIAL-ANSWER-EVENT/V4",
                        &action.payload(),
                    ));
                    let object = material_recovery_object_v2(self.transport.domain, action, provider.bond, |b, m, ctx| {
                        (*b == provider.bond).then(|| provider.key.sign_with_context(m, ctx).to_vec())
                    })
                    .ok_or("provider answer signing failed")?;
                    if !matches!(
                        api.palw_object_rehearsal_v1(&object),
                        Some(kaspa_consensus_core::palw_producer_v2::PalwObjectRehearsalV1::Accepted)
                    ) {
                        continue;
                    }
                    additional.insert(
                        event,
                        NativeCarrierPurposeV4::Answer {
                            provider: provider.bond,
                            stream: checkpoint.stream,
                            sequence,
                            event,
                            deadline_daa: c.deadline_daa,
                        },
                        &object,
                    )?;
                    break 'contracts;
                }
            }
        }
        // A release carrier is generated only when the exact native acceptance
        // layer agrees that its real physical-payment+W tail is finalized. It
        // does not create a shorter local service deadline or settle money.
        let mut queue = self.additional_carriers.lock().map_err(|_| "release queue poisoned")?;
        for row in &rows {
            let Some(tail) = row.tail_block else { continue };
            if !self.providers.iter().any(|p| row.providers.iter().any(|r| r.provider == p.bond)) {
                continue;
            }
            let action = kaspa_consensus_core::palw_monetary_material_v1::PalwMonetaryMaterialActionV1::Release {
                root: row.terms.root,
                consent: row.consent,
                tail_block: tail,
            };
            let object = Object::MonetaryMaterialActionV1 { action: Box::new(action), signer: row.terms.sponsor, signature: vec![] };
            if !matches!(
                api.palw_object_rehearsal_v1(&object),
                Some(kaspa_consensus_core::palw_producer_v2::PalwObjectRehearsalV1::Accepted)
            ) {
                continue;
            }
            let id = Hash64::from_bytes(misaka_palw_kernel::hash::object_id(b"MISAKA/PALW/RELEASE-CARRIER/V4", &row.terms.root));
            queue.replace_release_proposal(
                id,
                NativeCarrierPurposeV4::Release { root: row.terms.root, consent: row.consent, tail },
                &object,
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Cryptographic payload boundary only: neither these signatures nor the
    /// synthetic answer demonstrate native payment, a court verdict or finality.
    #[test]
    fn provider_event_signature_binds_immutable_deadline_and_stream() {
        use kaspa_consensus_core::{
            palw_material_release_recovery_v2::PalwMaterialRecoveryAnswerV4,
            palw_monetary_material_v1::PalwMonetaryMaterialActionV1 as M, tx::TransactionOutpoint,
        };
        let h = Hash64::from_u64_word;
        let provider = PalwBondKeyV2(TransactionOutpoint::new(h(1), 0));
        let key = kaspa_pq_validator_core::ValidatorKey::from_seed([7; 32]);
        let action = Recovery::AnswerAndCheckpoint {
            answer: Box::new(PalwMaterialRecoveryAnswerV4::Kernel {
                root: h(2),
                provider,
                dependency: h(3),
                stage: 0,
                position: 1,
                bytes: vec![4],
            }),
            stream: h(5),
            previous: h(6),
            sequence: 7,
            challenge_deadline_daa: 100,
        };
        let object =
            material_recovery_object_v2(h(9), action, provider, |_, m, c| Some(key.sign_with_context(m, c).to_vec())).unwrap();
        let Object::MonetaryMaterialActionV1 { action, signature, .. } = object else { panic!() };
        let payload = borsh::to_vec(&action).unwrap();
        assert!(verify(key.public_key(), message(h(9), 242, &provider, &payload).as_byte_slice(), &signature, CONTEXT));
        let mut changed = action.as_ref().clone();
        let M::RecoveryV2(inner) = &mut changed else { panic!() };
        let Recovery::AnswerAndCheckpoint { challenge_deadline_daa, .. } = &mut **inner else { panic!() };
        *challenge_deadline_daa += 1;
        assert!(!verify(
            key.public_key(),
            message(h(9), 242, &provider, &borsh::to_vec(&changed).unwrap()).as_byte_slice(),
            &signature,
            CONTEXT
        ));
        assert_eq!(borsh::to_vec(&ProviderTransportPurposeV1::NativeReadyV3).unwrap(), vec![0]);
        assert_eq!(borsh::to_vec(&ProviderTransportPurposeV1::RecoveryEnrollmentV4).unwrap(), vec![1]);
    }
    #[test]
    fn enrollment_request_and_response_nested_bombs_are_rejected_before_decode() {
        let mut request = 4u16.to_le_bytes().to_vec();
        request.extend([0u8; 64 + 8 + 64 + 64 + 68]);
        request.extend(u32::MAX.to_le_bytes());
        assert!(preflight_native_value_v1::<Request>(&request[..], request.len() as u64).is_err());
        let mut response = vec![0];
        response.extend(u32::MAX.to_le_bytes());
        assert!(preflight_native_value_v1::<Response>(&response[..], response.len() as u64).is_err());
    }
}
