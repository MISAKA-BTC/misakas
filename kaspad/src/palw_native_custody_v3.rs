//! **RFC-0008 PAV3: the in-node producer's half of the native custody exchange** (lead default B, 2026-10-15, pending user
//! review; unverified source — nothing here has been compiled or run).
//!
//! Past `palw_monetary_material_v1` every native attempt except the BASE floor class must be the owned signed custody form
//! (`palw_attempt_material_v3`). The order this module fixes, each step refusing rather than degrading:
//!
//! 1. The producer selects the canonical prior session from its OWN node's coherent tip read
//!    (`ConsensusApi::palw_native_session_v3`, the one `select_native_session_v3` rule `verify_prior` re-runs), refusing a roster
//!    with fewer than two distinct provider operators or one that includes the executor's own operator.
//! 2. Both draws are decided on the PAV3 priced ticket of a PROVISIONAL envelope ([`palw_native_draw_envelope_v3`]). The
//!    priced execution covers session, consent, roster and the immutable execution — never the manifest or any signature — so
//!    the actual capture bundle is built only for a won draw, and the final envelope's ticket is re-checked equal.
//! 3. On a win the producer obtains the material authority from the Consensus API, builds the actual typed bundle from its own
//!    capture with its resolved backend ([`palw_native_bundle_v3`]), drafts Ready, and asks the roster's providers to sign
//!    through a [`NativeReadyExchangeV3`] transport. Each provider rebuilds the bundle itself, durably publishes it, and only
//!    then signs `Ready::id`.
//! 4. Every returned signature is checked against the roster's registered keys before the executor seals.
//!
//! Ordinary kaspad has no Args path that configures an exchange: its producer keeps
//! [`crate::palw_producer::PALW_PRODUCER_NATIVE_CUSTODY_HOLD`]. The only wiring is the feature-gated private-integration
//! constructor's (`private_integration::native_provider`): in-process, or the ML-DSA-87 authenticated provider envelope
//! (`misaka_palw_remote::provider_transport_v1`) over a loopback socket by default or between hosts with
//! `--private-probe-native-network=remote` (2026-10-15 round 3).
use kaspa_consensus_core::{
    BlockHash,
    header::Header,
    palw_attempt_material_v3::{
        PALW_MATERIAL_READY_CONTEXT_V3, PalwAttemptMaterialEnvelopeV3, PalwNativeMaterialReadyV3, draft_native_ready_v3,
        verify_ready_signature_v3,
    },
    palw_attempt_v2::{PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2},
    palw_backend::{PalwClaimRootsV1, PalwExecutionBackendV1},
    palw_material_capacity_v1::{
        PalwHeldMaterialBundleV1, PalwMaterialManifestV1, PalwMaterialNativeAuthorityV1, PalwMaterialScopeV1,
    },
    palw_monetary_material_v1::PalwMonetaryMaterialRowV1,
    palw_state_v2::PalwBondKeyV2,
};
use kaspa_hashes::Hash64;
use std::{collections::BTreeMap, sync::Arc};

/// Wire version of [`PalwNativeReadyRequestV3`] / [`PalwNativeReadyResponseV3`].
pub const PALW_NATIVE_READY_EXCHANGE_VERSION_V3: u16 = 3;

/// **What a producer sends its session's providers.** Everything a provider needs to derive the bundle ITSELF and to obtain
/// the material authority from its own Consensus API: the pre-entropy checkpoint (a direct parent of `header`), the candidate
/// header at the attempt's nonce, the envelope carrying the signed base attempt and the exact Ready (its provider signatures
/// and seal are placeholders — authority never reads them), and the capture. `prompt` is a cross-check only: a provider
/// re-derives the prompt from the anchor with its own backend and refuses a different one.
#[derive(Clone, Debug, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwNativeReadyRequestV3 {
    pub version: u16,
    pub checkpoint: BlockHash,
    pub header: Header,
    pub envelope: PalwAttemptMaterialEnvelopeV3,
    pub capture: Vec<u8>,
    pub prompt: Vec<u32>,
}

/// A provider's answer: its signatures over `Ready::id` (each after durable publication), or why it signed nothing.
#[derive(Clone, Debug, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PalwNativeReadyResponseV3 {
    Signed { version: u16, signatures: Vec<(PalwBondKeyV2, Vec<u8>)> },
    Refused { version: u16, reason: String },
}

/// **The producer↔provider transport.** Synchronous on purpose: the producer calls it from a blocking task, and a provider's
/// work (rebuilding the typed bundle, fsync) is CPU and disk, not await. An implementation returns signatures only from
/// providers that durably published the bundle they rebuilt; the producer checks every one against the registered roster key.
pub trait NativeReadyExchangeV3: Send + Sync {
    /// For logs and the hold sentence.
    fn describe(&self) -> String;
    fn request_signatures(
        &self,
        request: &PalwNativeReadyRequestV3,
    ) -> Result<kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1<BTreeMap<PalwBondKeyV2, Vec<u8>>>, String>;
    fn recovery_provider_roster_v4(&self) -> &[PalwBondKeyV2] {
        &[]
    }
    /// Complete actual kernel material must be durable at every named provider
    /// before its atomic monetary/recovery carrier can be signed and queued.
    /// The caller still waits for its own positive native admission afterwards.
    fn ingest_kernel_material_v4(
        &self,
        _material: &kaspa_consensus_core::palw_prospective_kernel_material_v4::PalwProspectiveKernelMaterialRequestV4,
        _ingress: &kaspa_consensus_core::palw_kernel_material_enrollment_v4::PalwKernelMaterialIngressOwnedV4,
    ) -> Result<(), String> {
        Err("explicit V4 kernel material ingress unavailable".into())
    }
    /// Read permission/data identity only. The caller obtains the opaque native
    /// authority with its own explicitly configured registered checker key.
    fn fetch_kernel_material_unit_v4(
        &self,
        _query: &kaspa_consensus_core::palw_kernel_material_query_v4::PalwKernelMaterialUnitQueryV4,
        _authority: &kaspa_consensus_core::palw_kernel_material_query_v4::PalwKernelMaterialQueryAuthorityV4,
    ) -> Result<kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1<Vec<u8>>, String> {
        Err("explicit authenticated kernel material reader unavailable".into())
    }
    /// Legacy adapters cannot attribute their String error to a provider.
    fn fetch_kernel_material_unit_classified_v4(
        &self,
        query: &kaspa_consensus_core::palw_kernel_material_query_v4::PalwKernelMaterialUnitQueryV4,
        authority: &kaspa_consensus_core::palw_kernel_material_query_v4::PalwKernelMaterialQueryAuthorityV4,
    ) -> Result<
        kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1<Vec<u8>>,
        kaspa_consensus_core::palw_kernel_material_query_v4::PalwKernelMaterialFetchErrorV4,
    > {
        self.fetch_kernel_material_unit_v4(query, authority)
            .map_err(kaspa_consensus_core::palw_kernel_material_query_v4::PalwKernelMaterialFetchErrorV4::Unknown)
    }
    /// Queue a fully signed prospective contract; the caller must wait for its
    /// OWN coherent native prior-state admission before any execution/draw.
    fn ensure_recovery_session_v4(
        &self,
        _quote: &kaspa_consensus_core::palw_material_session_enrollment_v4::PalwNativeRecoverySessionQuoteV4,
    ) -> Result<(), String> {
        Err("V4 recovery enrollment transport unavailable".into())
    }
    /// The producer's call: `roster_keys` are the registered provider keys its own Consensus API authority returned, which an
    /// authenticated network transport verifies the provider responses with (2026-10-15 round 3). The in-process service
    /// needs none and keeps [`Self::request_signatures`].
    fn request_signatures_authenticated(
        &self,
        request: &PalwNativeReadyRequestV3,
        roster_keys: &[(PalwBondKeyV2, Vec<u8>)],
    ) -> Result<kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1<BTreeMap<PalwBondKeyV2, Vec<u8>>>, String> {
        let _ = roster_keys;
        self.request_signatures(request)
    }
}

/// The configured exchange, as [`crate::palw_producer::PalwProducerConfig`] carries it.
#[derive(Clone)]
pub struct PalwNativeExchangeHandleV3(pub Arc<dyn NativeReadyExchangeV3>);
impl std::fmt::Debug for PalwNativeExchangeHandleV3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PalwNativeExchangeHandleV3({})", self.0.describe())
    }
}

/// What the daemon hands a native-custody wiring hook once the consensus manager exists.
pub struct PalwNativeCustodyContextV3 {
    pub consensus_manager: Arc<kaspa_consensusmanager::ConsensusManager>,
    /// Used only by an explicitly configured private recovery fee adapter.
    pub flow_context: Arc<kaspa_p2p_flows::flow_context::FlowContext>,
    pub tick_service: Arc<kaspa_core::task::tick::TickService>,
    /// Explicit private-v4 prospective roster; empty in the original V3 hook.
    pub recovery_provider_roster_v4: Vec<kaspa_consensus_core::palw_state_v2::PalwBondKeyV2>,
    pub config: Arc<kaspa_consensus_core::config::Config>,
    /// The node's per-network data directory (`<appdir>/<network>`).
    pub data_dir: std::path::PathBuf,
    pub class_artifacts: Vec<std::path::PathBuf>,
    pub class_cache_bytes: u64,
    pub chain_classes: bool,
    /// This node's producer key/bond, when it runs one — a provider refuses to be the same key.
    pub producer_key_path: Option<String>,
    pub producer_bond: Option<String>,
}

/// A wiring hook: may start a provider service and returns the exchange this node's producer uses, if any. Only the
/// feature-gated private constructor builds one; ordinary kaspad passes `None`.
pub type PalwNativeCustodyHookV3 =
    Box<dyn FnOnce(PalwNativeCustodyContextV3) -> Result<Option<PalwNativeExchangeHandleV3>, String> + Send>;

/// **Is the owned PAV3 custody form required for this producer's next attempt?** The same condition the hold read: past
/// `palw_monetary_material_v1`, every class but the BASE floor (lead default 2026-10-15). One spelling for the hold and the
/// exchange path.
pub fn palw_native_custody_in_force_v3(params: &kaspa_consensus_core::config::params::Params, is_base_class: bool, daa: u64) -> bool {
    params.palw_monetary_material_v1.as_ref().is_some_and(|fence| fence.activation.is_active(daa)) && !is_base_class
}

/// **The roster the producer will ask, refused before any work is spent on it**: at least two distinct provider operators,
/// none of them the executor's own (self-custody is no custody). `operators` is the session read's uncharged roster.
pub fn palw_native_roster_refusal_v3(executor_operator: Hash64, operators: &[(PalwBondKeyV2, Hash64)]) -> Option<String> {
    let distinct: std::collections::BTreeSet<_> = operators.iter().map(|(_, operator)| *operator).collect();
    if distinct.len() < 2 {
        return Some(format!("the native session roster has {} distinct provider operator(s); two are required", distinct.len()));
    }
    if distinct.contains(&executor_operator) {
        return Some("a native session provider is the executor's own operator; self-custody is refused".into());
    }
    None
}

/// The one-scope manifest the DRAW is priced with: the session's root and codec and this exact execution, with unit
/// placeholders for the bundle's bytes/units/content. `priced_execution` reads none of those, so the ticket is the final
/// envelope's ticket (re-checked after assembly); the placeholder is never signed or published.
pub fn palw_native_draw_manifest_v3(
    row: &PalwMonetaryMaterialRowV1,
    attempt: &PalwAttemptUnsignedV2,
) -> Result<PalwMaterialManifestV1, String> {
    let session = row.terms.native_session.as_ref().ok_or("row carries no prospective native session")?;
    Ok(PalwMaterialManifestV1 {
        root: row.terms.root,
        scopes: vec![PalwMaterialScopeV1 {
            claim: attempt_id_v2(attempt),
            commitment: attempt.execution_root,
            codec: session.codec,
            bytes: 1,
            units: 1,
            content: Hash64::from_u64_word(1),
        }],
    })
}

/// A PAV3 envelope with placeholder provider signatures and seal of the real lengths — the shape the header-stage gate sees at
/// the draw, and the carriage a provider request names (authority never reads the placeholders).
pub fn palw_native_provisional_envelope_v3(
    base: PalwAttemptEnvelopeV2,
    ready: PalwNativeMaterialReadyV3,
) -> PalwAttemptMaterialEnvelopeV3 {
    let dummy = vec![0u8; kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN];
    let providers = ready.providers.len();
    PalwAttemptMaterialEnvelopeV3 { version: 3, base, ready, provider_signatures: vec![dummy.clone(); providers], seal: dummy }
}

/// The draw-time envelope: the canonical session's Ready over the draw manifest, on the attempt with a placeholder signature.
pub fn palw_native_draw_envelope_v3(
    row: &PalwMonetaryMaterialRowV1,
    attempt: &PalwAttemptUnsignedV2,
    anchor: Hash64,
) -> Result<PalwAttemptMaterialEnvelopeV3, String> {
    let ready = draft_native_ready_v3(row, attempt, anchor, palw_native_draw_manifest_v3(row, attempt)?)?;
    let base = PalwAttemptEnvelopeV2 {
        attempt: attempt.clone(),
        signature: vec![0u8; kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
    };
    let envelope = palw_native_provisional_envelope_v3(base, ready);
    envelope.validate_shape()?;
    Ok(envelope)
}

/// The claim roots an attempt capture is answered under: the attempt's own roots, the job anchor its header derives, and the
/// attempt draw at its DAA (`work_leaves` is 0 for the attempt lane).
pub fn palw_native_roots_v3(attempt: &PalwAttemptUnsignedV2, anchor: Hash64, prefill_draw: bool) -> PalwClaimRootsV1 {
    PalwClaimRootsV1 {
        execution_root: attempt.execution_root,
        trace_root: attempt.trace_root,
        anchor,
        attempt_draw: Some(prefill_draw),
        output_root: None,
        job_pin: None,
    }
}

/// **The actual typed bundle and its one-scope manifest**, built by the backend codec from the capture (never a RAM estimate
/// or a model label). Producer and provider call this one function; each with its own backend and capture.
#[allow(clippy::too_many_arguments)]
pub fn palw_native_bundle_v3(
    backend: &dyn PalwExecutionBackendV1,
    row: &PalwMonetaryMaterialRowV1,
    attempt: &PalwAttemptUnsignedV2,
    anchor: Hash64,
    capture: &[u8],
    prompt: &[u32],
    prefill_draw: bool,
    authority: PalwMaterialNativeAuthorityV1,
) -> Result<(PalwHeldMaterialBundleV1, PalwMaterialManifestV1), String> {
    if authority.execution_root != attempt.execution_root
        || authority.class_id != attempt.class_id
        || authority.artifact_root != attempt.artifact_root
    {
        return Err("native material authority names another execution/class/artifact".into());
    }
    let claim = attempt_id_v2(attempt);
    let max = row.terms.max_material_bytes;
    let bundle = PalwHeldMaterialBundleV1::from_capture(
        claim,
        backend,
        capture,
        prompt,
        palw_native_roots_v3(attempt, anchor, prefill_draw),
        0,
        authority,
        max,
    )
    .map_err(|e| format!("the capture's typed bundle: {e}"))?;
    let scope = bundle.verify(authority, max).map_err(|e| format!("the capture's typed bundle: {e}"))?;
    let manifest = PalwMaterialManifestV1 { root: row.terms.root, scopes: vec![scope] };
    manifest.validate(max).map_err(|e| format!("the capture's manifest: {e}"))?;
    Ok((bundle, manifest))
}

/// The prompt an anchor implies, as the u32 ids every capture builder takes.
pub fn palw_native_prompt_v3(backend: &dyn PalwExecutionBackendV1, anchor: Hash64) -> Result<Vec<u32>, String> {
    let (_, prompt) = backend.job_for_anchor(anchor)?;
    prompt.iter().map(|id| u32::try_from(*id).map_err(|_| format!("prompt id {id} does not fit a u32"))).collect()
}

/// **Every roster signature, checked before the executor seals**: exactly the roster, each verifying over `Ready::id` under
/// the READY context with the provider's registered key (`provider_keys`, from the Consensus API's authority).
pub fn palw_native_check_signatures_v3(
    ready: &PalwNativeMaterialReadyV3,
    provider_keys: &[(PalwBondKeyV2, Vec<u8>)],
    signatures: &BTreeMap<PalwBondKeyV2, Vec<u8>>,
) -> Result<(), String> {
    if signatures.len() != ready.providers.len() || !ready.providers.iter().all(|p| signatures.contains_key(p)) {
        return Err("the provider signature set differs from the session roster".into());
    }
    let id = ready.id();
    for provider in &ready.providers {
        let key =
            provider_keys.iter().find(|(p, _)| p == provider).map(|(_, key)| key).ok_or("a roster provider has no registered key")?;
        if !verify_ready_signature_v3(key, id.as_byte_slice(), &signatures[provider], PALW_MATERIAL_READY_CONTEXT_V3) {
            return Err(format!(
                "provider {}:{} returned a Ready signature that does not verify",
                provider.0.transaction_id, provider.0.index
            ));
        }
    }
    Ok(())
}

/// The final draw check after assembly: the sealed carriage prices exactly what the provisional one drew.
pub fn palw_native_same_draw_v3(
    provisional: &PalwAttemptMaterialEnvelopeV3,
    sealed: &PalwAttemptMaterialEnvelopeV3,
    anchor: Hash64,
) -> Result<(), String> {
    if provisional.priced_execution(anchor) != sealed.priced_execution(anchor) {
        return Err("the sealed native carriage prices another draw than the one won; not publishing".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::{
        palw_attempt_material_v3::{PalwNativeMaterialBindingV3, PalwNativeMaterialSessionV3},
        palw_material_capacity_v1::PalwMaterialCodecV1,
        tx::TransactionOutpoint,
    };

    fn bond(word: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(word), 0))
    }

    fn base() -> PalwAttemptEnvelopeV2 {
        use kaspa_consensus_core::palw_attempt_v2::{PALW_ATTEMPT_V2_VERSION, attempt_trace_manifest_root_v1, challenge_v2};
        let h = Hash64::from_u64_word;
        let executor = TransactionOutpoint::new(h(90), 1);
        PalwAttemptEnvelopeV2 {
            attempt: PalwAttemptUnsignedV2 {
                version: PALW_ATTEMPT_V2_VERSION,
                network_domain: h(1),
                challenge: challenge_v2(h(1), h(2), 7, 1, h(3), &executor),
                class_id: h(3),
                executor_bond: executor,
                executor_pubkey: vec![0x11; kaspa_consensus_core::mldsa87_primitives::MLDSA87_PUBKEY_LEN],
                operator_id: h(4),
                artifact_root: h(5),
                trace_root: h(31),
                output_root: h(32),
                execution_root: h(41),
                pwu: 1,
                trace_manifest_root: attempt_trace_manifest_root_v1(h(31), 1),
                trace_chunk_count: 1,
                trace_retention_daa: 999_999,
            },
            signature: vec![0x5A; kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
        }
    }

    #[test]
    fn the_roster_needs_two_distinct_operators_and_none_of_them_the_executors() {
        let executor = Hash64::from_u64_word(1);
        let (a, b) = (Hash64::from_u64_word(2), Hash64::from_u64_word(3));
        assert!(palw_native_roster_refusal_v3(executor, &[(bond(10), a), (bond(11), b)]).is_none());
        assert!(palw_native_roster_refusal_v3(executor, &[(bond(10), a), (bond(11), a)]).unwrap().contains("distinct"));
        assert!(palw_native_roster_refusal_v3(executor, &[(bond(10), a)]).is_some());
        assert!(palw_native_roster_refusal_v3(executor, &[(bond(10), a), (bond(11), executor)]).unwrap().contains("self-custody"));
    }

    #[test]
    fn the_hold_and_the_exchange_read_one_condition_and_the_floor_is_exempt() {
        let mut params = kaspa_consensus_core::config::params::TESTNET_PARAMS.clone();
        params.palw_monetary_material_v1 = None;
        assert!(!palw_native_custody_in_force_v3(&params, false, u64::MAX));
        let source = include_str!("palw_producer.rs");
        let produce = &source[source.find("async fn produce_one(").expect("produce_one")..];
        assert!(produce.contains("palw_native_custody_in_force_v3("), "the producer reads the one condition");
        assert!(produce.contains("PALW_PRODUCER_NATIVE_CUSTODY_HOLD"), "and holds, explicitly, without an exchange");
    }

    /// The draw is priced without the manifest: a placeholder manifest and the actual one give the same ticket, and a provider
    /// signature or seal byte moves neither.
    #[test]
    fn the_draw_manifest_prices_the_same_ticket_as_the_actual_manifest() {
        let base = base();
        let attempt = base.attempt.clone();
        let claim = attempt_id_v2(&attempt);
        let session = PalwNativeMaterialSessionV3 {
            version: 3,
            class_id: attempt.class_id,
            artifact_root: attempt.artifact_root,
            executor: PalwBondKeyV2(attempt.executor_bond),
            operator: attempt.operator_id,
            job_policy: kaspa_consensus_core::palw_attempt_material_v3::native_job_policy_v3(),
            codec: PalwMaterialCodecV1::NativeHeldComplete,
            max_units: 10,
            one_claim_cap: 1,
            admit_from_daa: 0,
            admit_until_daa: u64::MAX,
        };
        let anchor = Hash64::from_u64_word(5);
        let ready = |manifest: PalwMaterialManifestV1| PalwNativeMaterialReadyV3 {
            version: 3,
            session: Hash64::from_u64_word(7),
            consent: Hash64::from_u64_word(8),
            binding: PalwNativeMaterialBindingV3 {
                claim,
                execution_key: kaspa_consensus_core::palw_attempt_v2::execution_commitment_v3(&attempt, anchor),
                manifest,
            },
            providers: vec![bond(4), bond(5)],
        };
        let scope = |bytes, units, content| PalwMaterialScopeV1 {
            claim,
            commitment: attempt.execution_root,
            codec: session.codec,
            bytes,
            units,
            content: Hash64::from_u64_word(content),
        };
        let draw = palw_native_provisional_envelope_v3(
            base.clone(),
            ready(PalwMaterialManifestV1 { root: Hash64::from_u64_word(7), scopes: vec![scope(1, 1, 1)] }),
        );
        let actual = palw_native_provisional_envelope_v3(
            base,
            ready(PalwMaterialManifestV1 { root: Hash64::from_u64_word(7), scopes: vec![scope(4096, 9, 77)] }),
        );
        assert_eq!(draw.class_ticket(anchor), actual.class_ticket(anchor));
        assert!(palw_native_same_draw_v3(&draw, &actual, anchor).is_ok());
        assert_eq!(draw.encode_wire().len(), actual.encode_wire().len(), "the draw sees the final wire size");
        let mut signed = actual.clone();
        signed.provider_signatures[0][0] ^= 1;
        signed.seal[0] ^= 1;
        assert!(palw_native_same_draw_v3(&draw, &signed, anchor).is_ok());
    }

    #[test]
    fn a_signature_set_that_is_not_exactly_the_verifying_roster_is_refused() {
        let base = base();
        let claim = attempt_id_v2(&base.attempt);
        let ready = PalwNativeMaterialReadyV3 {
            version: 3,
            session: Hash64::from_u64_word(7),
            consent: Hash64::from_u64_word(8),
            binding: PalwNativeMaterialBindingV3 {
                claim,
                execution_key: Hash64::from_u64_word(3),
                manifest: PalwMaterialManifestV1 { root: Hash64::from_u64_word(7), scopes: vec![] },
            },
            providers: vec![bond(4), bond(5)],
        };
        let keys: Vec<_> = [4u8, 5].iter().map(|n| libcrux_ml_dsa::ml_dsa_87::generate_key_pair([*n; 32])).collect();
        let roster: Vec<_> = ready.providers.iter().zip(&keys).map(|(p, k)| (*p, k.verification_key.as_ref().to_vec())).collect();
        let sign = |k: &libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair| {
            libcrux_ml_dsa::ml_dsa_87::sign(&k.signing_key, ready.id().as_byte_slice(), PALW_MATERIAL_READY_CONTEXT_V3, [9u8; 32])
                .unwrap()
                .as_ref()
                .to_vec()
        };
        let mut signatures: BTreeMap<_, _> = ready.providers.iter().zip(&keys).map(|(p, k)| (*p, sign(k))).collect();
        palw_native_check_signatures_v3(&ready, &roster, &signatures).unwrap();
        let swapped: BTreeMap<_, _> = ready.providers.iter().zip(keys.iter().rev()).map(|(p, k)| (*p, sign(k))).collect();
        assert!(palw_native_check_signatures_v3(&ready, &roster, &swapped).is_err(), "a signature by another roster key");
        signatures.remove(&bond(5));
        assert!(palw_native_check_signatures_v3(&ready, &roster, &signatures).is_err(), "a missing provider");
    }
}

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

/// Canonical map construction without an intermediate unreserved BTree map.
/// The source signature owner remains alive while its exact map frame is
/// encoded, preflighted, and decoded into the returned independent owner.
pub fn palw_native_signature_map_owned_v3(
    signatures: &[(PalwBondKeyV2, Vec<u8>)],
) -> Result<kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1<BTreeMap<PalwBondKeyV2, Vec<u8>>>, String> {
    use kaspa_consensus_core::palw_native_resource_v1::{
        preflight_native_value_v1, reserve_native_requests_v1, retain_native_value_decode_v1,
    };
    if signatures.len() > kaspa_consensus_core::palw_attempt_material_v3::PALW_MATERIAL_MAX_PROVIDERS_V3
        || signatures.windows(2).any(|r| r[0].0 >= r[1].0)
    {
        return Err("signature map roster is oversized, repeated or reordered".into());
    }
    let len = signatures
        .iter()
        .try_fold(4u64, |n, (b, s)| n.checked_add(borsh::object_length(&(b, s)).ok()? as u64))
        .ok_or("signature map frame overflow")?;
    let _raw =
        reserve_native_requests_v1(len.checked_mul(2).ok_or("signature map raw ownership overflow")?).map_err(|e| e.to_string())?;
    let mut bytes = Vec::with_capacity(usize::try_from(len).map_err(|_| "signature frame exceeds address space")?);
    bytes.extend((signatures.len() as u32).to_le_bytes());
    for row in signatures {
        borsh::to_writer(&mut bytes, row).map_err(|e| e.to_string())?;
    }
    preflight_native_value_v1::<BTreeMap<PalwBondKeyV2, Vec<u8>>>(&bytes[..], bytes.len() as u64).map_err(|e| e.to_string())?;
    retain_native_value_decode_v1(&bytes, || borsh::from_slice::<BTreeMap<PalwBondKeyV2, Vec<u8>>>(&bytes)).map_err(|e| e.to_string())
}
