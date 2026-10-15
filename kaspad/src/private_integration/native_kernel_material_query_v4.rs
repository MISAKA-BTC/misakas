//! Authenticated read of one retained unit. Party/byte authentication grants no
//! model-computation credit and does not replace randomly selected approval.
use super::*;
use kaspa_consensus_core::{
    palw_kernel_material_query_v4::{
        PALW_KERNEL_MATERIAL_QUERY_FRAME_BYTES_V4 as FRAME, PALW_KERNEL_MATERIAL_QUERY_REQUEST_BYTES_V4 as REQUEST,
        PalwKernelMaterialQueryAuthorityV4 as Authority, PalwKernelMaterialUnitQueryV4 as Query,
        PalwKernelMaterialUnitResponseV4 as Response, kernel_material_query_response_v4, take_kernel_material_query_bytes_v4,
    },
    palw_material_capacity_v1::PalwMonetaryMaterialDemandV1,
    palw_native_resource_v1::{
        NativeRetainedV1, preflight_native_value_v1, reserve_native_requests_v1, retain_native_value_decode_v1,
    },
};
impl AuthenticatedSocketExchangeV3 {
    /// Separate explicit checker identity. No producer/provider key is silently
    /// reused and no caller-supplied public key becomes native authority.
    pub(crate) fn kernel_reader_v4(
        addr: SocketAddr,
        policy: ProviderTransportPolicyV1,
        requester: PalwBondKeyV2,
        key: kaspa_pq_validator_core::ValidatorKey,
    ) -> Self {
        Self { addr, policy, sender: requester, key, recovery_roster: Vec::new() }
    }
    pub(super) fn fetch_kernel_v4(&self, query: &Query, authority: &Authority) -> Result<NativeRetainedV1<Vec<u8>>, String> {
        query.validate()?;
        if authority.query() != query
            || self.sender != authority.requester()
            || self.key.public_key() != authority.requester_pubkey()
            || self.policy.domain != authority.network()
        {
            return Err("kernel query differs from own native authority/checker key/network".into());
        }
        // Raw response, outer and inner Borsh owners have distinct tickets.
        // This workspace covers canonical output buffers, key/signature frames,
        // and verification scratch while those typed owners remain live.
        let cap = authority.max_response_frame_bytes();
        let _wire = reserve_native_requests_v1(cap.checked_mul(6).ok_or("kernel query wire overflow")?).map_err(|e| e.to_string())?;
        let body = borsh::to_vec(query).map_err(|e| e.to_string())?;
        if body.len() as u64 != REQUEST {
            return Err("kernel query fixed wire differs".into());
        }
        let request = provider_transport_request_v1(
            ProviderTransportPurposeV1::KernelMaterialQueryV4,
            authority.network(),
            self.sender,
            self.key.public_key().to_vec(),
            vec![query.provider],
            request_nonce_v3(&body),
            now_unix_ms(),
            authority.daa_score(),
            body,
            |m, c| Ok(self.key.sign_with_context(m, c).to_vec()),
        )?;
        let mut socket = TcpStream::connect_timeout(&self.addr, Duration::from_secs(10)).map_err(|e| e.to_string())?;
        socket.set_read_timeout(Some(PRIVATE_NATIVE_EXCHANGE_TIMEOUT_V3)).map_err(|e| e.to_string())?;
        socket.set_write_timeout(Some(PRIVATE_NATIVE_EXCHANGE_TIMEOUT_V3)).map_err(|e| e.to_string())?;
        write_frame_v3(&mut socket, &borsh::to_vec(&request).map_err(|e| e.to_string())?)?;
        let frame = read_frame_capped_v4(&mut socket, cap)?;
        preflight_native_value_v1::<ProviderTransportResponseV1>(&frame, frame.len() as u64).map_err(|e| e.to_string())?;
        let envelope = retain_native_value_decode_v1(&frame, || borsh::from_slice::<ProviderTransportResponseV1>(&frame))
            .map_err(|e| e.to_string())?;
        let keys = [(query.provider, authority.provider_pubkey().to_vec())];
        let responders = provider_transport_verify_response_v1(&envelope, &request.header, &keys, &self.policy, now_unix_ms())?;
        if responders.as_slice() != [query.provider] {
            return Err("kernel query provider differs".into());
        }
        preflight_native_value_v1::<Response>(&envelope.body, envelope.body.len() as u64).map_err(|e| e.to_string())?;
        let answer = retain_native_value_decode_v1(&envelope.body, || borsh::from_slice::<Response>(&envelope.body))
            .map_err(|e| e.to_string())?;
        // Transfers the admitted byte owner without cloning or discarding its
        // lease when the response envelope and frame are dropped.
        let bytes = take_kernel_material_query_bytes_v4(answer, query)?;
        if bytes.len() as u64 > authority.scope().bytes {
            return Err("kernel query unit exceeds the entire accepted signed scope".into());
        }
        Ok(bytes)
    }
}
struct QueryAnswerV4 {
    response: NativeRetainedV1<Response>,
    cap: u64,
    _workspace: Option<kaspa_consensus_core::palw_native_resource_v1::NativeDemandLeaseV1>,
}
impl LoopbackNativeProviderV3 {
    fn kernel_query_v4(&self, request: &ProviderTransportRequestV1, query: &Query) -> Result<QueryAnswerV4, String> {
        let api = self.consensus_manager.consensus().unguarded_session_blocking();
        let authority = api.palw_kernel_material_query_authority_v4(query, request.header.sender, &request.header.sender_pubkey)?;
        if request.header.domain != authority.network() || request.header.recipients.as_slice() != [query.provider] {
            return Err("kernel query native network/provider differs".into());
        }
        let provider = self.providers.iter().find(|p| p.bond == query.provider).ok_or("requested provider not hosted")?;
        if provider.key.public_key() != authority.provider_pubkey() {
            return Err("kernel query provider native key differs".into());
        }
        let cap = authority.max_response_frame_bytes();
        let workspace = reserve_native_requests_v1(cap.checked_mul(6).ok_or("kernel query response workspace overflow")?)
            .map_err(|e| e.to_string())?;
        let answer = provider.store.lock().map_err(|_| "kernel query store poisoned")?.answer_material_exact_current_v4(
            query.consent,
            query.root,
            query.dependency,
            query.provider,
            provider.key.public_key(),
            PalwMonetaryMaterialDemandV1::Kernel { stage: query.stage, position: query.position },
            &*api,
        )?;
        // Recheck coherent current native authority after restore/build. A
        // reorg/replacement while reading fails closed; it is not slashable.
        api.palw_kernel_material_query_authority_v4(query, request.header.sender, &request.header.sender_pubkey)?;
        Ok(QueryAnswerV4 { response: kernel_material_query_response_v4(answer, query)?, cap, _workspace: workspace })
    }
}
pub(super) fn handle_query_v4(stream: &mut TcpStream, service: &LoopbackNativeProviderV3, request: &ProviderTransportRequestV1) {
    if request.body.len() as u64 != REQUEST {
        return;
    }
    let local = service.consensus_manager.consensus().unguarded_session().get_virtual_daa_score();
    let Ok(mut replay) = service.replay.lock() else { return };
    if provider_transport_verify_request_v1(
        request,
        ProviderTransportPurposeV1::KernelMaterialQueryV4,
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
    if preflight_native_value_v1::<Query>(&request.body, REQUEST).is_err() {
        return;
    }
    let Ok(query) = retain_native_value_decode_v1(&request.body, || borsh::from_slice::<Query>(&request.body)) else { return };
    if query.validate().is_err() || request.header.recipients.as_slice() != [query.provider] {
        return;
    }
    let Some(provider) = service.providers.iter().find(|p| p.bond == query.provider) else { return };
    // Small refusal framing is distinct from successful unit workspace. The
    // latter is reserved from the accepted scope, before obtaining raw bytes.
    let Ok(_refusal_workspace) = reserve_native_requests_v1(65536 * 6) else { return };
    let result = service.kernel_query_v4(request, &query);
    let refusal;
    let body = match &result {
        Ok(answer) => &*answer.response,
        Err(reason) => {
            if reason.len() > 1024 {
                return;
            }
            refusal = Response::Refused { query: *query, reason: reason.clone() };
            &refusal
        }
    };
    let signer = |m: &[u8], c: &[u8]| Ok(provider.key.sign_with_context(m, c).to_vec());
    let response = borsh::to_vec(body)
        .map_err(|e| e.to_string())
        .and_then(|bytes| {
            provider_transport_response_v1(
                &request.header,
                now_unix_ms(),
                bytes,
                vec![(query.provider, &signer as ProviderTransportSignerV1<'_>)],
            )
        })
        .and_then(|r| borsh::to_vec(&r).map_err(|e| e.to_string()));
    if let Ok(bytes) = response {
        let cap = result.as_ref().map(|r| r.cap).unwrap_or(65536).min(FRAME);
        if bytes.len() as u64 <= cap {
            let _ = write_frame_v3(stream, &bytes);
        }
    }
}
