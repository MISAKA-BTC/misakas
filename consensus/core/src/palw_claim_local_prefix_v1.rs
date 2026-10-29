//! RFC-0040 / ADR-0208: authenticated prefix provenance contracts.
//! These signed values authenticate identity and finite obligations, not model
//! computation. Admission, export adjudication and the rooted registry must all
//! be connected before the stage-2 fence can be armed.
use crate::{Hash64, palw_freeprompt_v3::PalwFpPrefixStateV1};
use borsh::{BorshDeserialize, BorshSerialize};

pub const PREFIX_EXPORT_CONTEXT_V1: &[u8] = b"misaka-palw/prefix-export/v1/mldsa87";
pub const PREFIX_USE_CONTEXT_V1: &[u8] = b"misaka-palw/prefix-use/v1/mldsa87";

/// Prospective source statement. No future block identity and no signature or
/// dependency wrapper is inside `base_commitment`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPrefixExportIntentV1 {
    pub version: u8,
    pub network: Hash64,
    /// 1 = chain free-prompt claim. Unknown namespaces are never aliases.
    pub source_namespace: u8,
    pub source_nonce: Hash64,
    pub base_commitment: Hash64,
    pub producer_key_id: Hash64,
    pub ordinal: u32,
    pub prefix: PalwFpPrefixStateV1,
    pub liability_until: u64,
    pub material_until: u64,
}

/// Consensus binds a prospective statement to its actual accepted cohort. This
/// value is an identity, not a producer assertion that admission occurred.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPrefixExportIdentityV1 {
    pub intent: Hash64,
    pub source_claim: Hash64,
    pub accepted_block: Hash64,
    pub accepted_daa: u64,
    pub accepted_blue_score: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPrefixUseIntentV1 {
    pub version: u8,
    pub network: Hash64,
    pub consumer_nonce: Hash64,
    pub base_commitment: Hash64,
    pub consumer_key_id: Hash64,
    pub source: PalwPrefixExportIdentityV1,
    pub prefix: PalwFpPrefixStateV1,
    pub hard_deadline: u64,
    pub material_until: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwPrefixDependencyRefusalV1 {
    #[error("unsupported or incomplete prefix provenance contract")]
    Contract,
    #[error("prefix provenance signature or producer key differs")]
    Signature,
    #[error("prefix provenance belongs to another network, base claim or export")]
    Identity,
    #[error("prefix provenance source is not causally prior to its consumer")]
    FutureSource,
    #[error("prefix consumer exceeds its source's finite signed liability or material bounds")]
    Retention,
}
use PalwPrefixDependencyRefusalV1 as R;

fn id<T: BorshSerialize>(domain: &[u8], value: &T) -> Hash64 {
    Hash64::from_bytes(misaka_palw_kernel::hash::object_id(domain, value))
}
fn signature(pubkey: &[u8], key_id: Hash64, message: Hash64, sig: &[u8], context: &[u8]) -> Result<(), R> {
    if crate::mldsa87_primitives::mldsa87_key_id(pubkey) != key_id
        || !crate::palw_attempt_material_v3::verify_ready_signature_v3(pubkey, message.as_byte_slice(), sig, context)
    {
        return Err(R::Signature);
    }
    Ok(())
}
impl PalwPrefixExportIntentV1 {
    pub fn id_v1(&self) -> Hash64 {
        id(b"misaka-palw/prefix-export/intent/v1", self)
    }
    pub fn check_v1(&self) -> Result<(), R> {
        if self.version != 1
            || self.source_namespace != 1
            || self.network == Hash64::default()
            || self.source_nonce == Hash64::default()
            || self.base_commitment == Hash64::default()
            || self.producer_key_id == Hash64::default()
            || self.prefix.is_genesis()
            || self.prefix.class_id == Hash64::default()
            || self.liability_until == 0
            || self.material_until < self.liability_until
        {
            return Err(R::Contract);
        }
        Ok(())
    }
    pub fn verify_signature_v1(&self, pubkey: &[u8], sig: &[u8]) -> Result<(), R> {
        self.check_v1()?;
        signature(pubkey, self.producer_key_id, self.id_v1(), sig, PREFIX_EXPORT_CONTEXT_V1)
    }
}
impl PalwPrefixExportIdentityV1 {
    pub fn id_v1(&self) -> Hash64 {
        id(b"misaka-palw/prefix-export/accepted/v1", self)
    }
}
impl PalwPrefixUseIntentV1 {
    pub fn id_v1(&self) -> Hash64 {
        id(b"misaka-palw/prefix-use/intent/v1", self)
    }
    pub fn check_v1(&self) -> Result<(), R> {
        if self.version != 1
            || self.network == Hash64::default()
            || self.consumer_nonce == Hash64::default()
            || self.base_commitment == Hash64::default()
            || self.consumer_key_id == Hash64::default()
            || self.source.intent == Hash64::default()
            || self.source.source_claim == Hash64::default()
            || self.source.accepted_block == Hash64::default()
            || self.prefix.is_genesis()
            || self.hard_deadline == 0
            || self.material_until < self.hard_deadline
        {
            return Err(R::Contract);
        }
        Ok(())
    }
    pub fn verify_signature_v1(&self, pubkey: &[u8], sig: &[u8]) -> Result<(), R> {
        self.check_v1()?;
        signature(pubkey, self.consumer_key_id, self.id_v1(), sig, PREFIX_USE_CONTEXT_V1)
    }
    /// Caller supplies the export row obtained from authenticated executed state,
    /// never the caller's copy of a producer-declared source identity. Every
    /// transitive edge must pass this same bound check at admission.
    pub fn check_source_v1(
        &self,
        export: &PalwPrefixExportIntentV1,
        accepted: &PalwPrefixExportIdentityV1,
        network: Hash64,
        consumer_base: Hash64,
        consumer_daa: u64,
        consumer_blue_score: u64,
    ) -> Result<(), R> {
        self.check_v1()?;
        export.check_v1()?;
        if self.network != network
            || export.network != network
            || self.base_commitment != consumer_base
            || self.source != *accepted
            || accepted.intent != export.id_v1()
            || self.prefix != export.prefix
        {
            return Err(R::Identity);
        }
        if accepted.accepted_daa > consumer_daa || accepted.accepted_blue_score >= consumer_blue_score {
            return Err(R::FutureSource);
        }
        if self.hard_deadline < consumer_daa
            || self.hard_deadline > export.liability_until
            || self.material_until > export.material_until
        {
            return Err(R::Retention);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn h(n: u8) -> Hash64 {
        Hash64::from_bytes([n; 64])
    }
    fn contracts() -> (PalwPrefixExportIntentV1, PalwPrefixExportIdentityV1, PalwPrefixUseIntentV1) {
        let e = PalwPrefixExportIntentV1 {
            version: 1,
            network: h(1),
            source_namespace: 1,
            source_nonce: h(2),
            base_commitment: h(3),
            producer_key_id: h(4),
            ordinal: 0,
            prefix: PalwFpPrefixStateV1 { state_root: h(5), prefix_tokens: 8, class_id: h(6) },
            liability_until: 100,
            material_until: 120,
        };
        let a = PalwPrefixExportIdentityV1 {
            intent: e.id_v1(),
            source_claim: h(7),
            accepted_block: h(8),
            accepted_daa: 10,
            accepted_blue_score: 20,
        };
        let c = PalwPrefixUseIntentV1 {
            version: 1,
            network: h(1),
            consumer_nonce: h(9),
            base_commitment: h(10),
            consumer_key_id: h(11),
            source: a,
            prefix: e.prefix,
            hard_deadline: 100,
            material_until: 120,
        };
        (e, a, c)
    }
    #[test]
    fn rfc40_prefix_contract_checks_real_source_identity_causality_and_finite_bounds() {
        let (e, a, c) = contracts();
        c.check_source_v1(&e, &a, h(1), h(10), 30, 40).unwrap();
        for mode in 0..8 {
            let mut bad = c.clone();
            match mode {
                0 => bad.network = h(30),
                1 => bad.base_commitment = h(30),
                2 => bad.source.accepted_block = h(30),
                3 => bad.source.source_claim = h(30),
                4 => bad.prefix.state_root = h(30),
                5 => bad.prefix.prefix_tokens += 1,
                6 => bad.hard_deadline = 101,
                _ => bad.material_until = 121,
            }
            assert!(bad.check_source_v1(&e, &a, h(1), h(10), 30, 40).is_err());
        }
        assert_eq!(c.check_source_v1(&e, &a, h(1), h(10), 9, 40), Err(R::FutureSource));
        assert_eq!(c.check_source_v1(&e, &a, h(1), h(10), 30, 20), Err(R::FutureSource));
        let mut replay = a;
        replay.accepted_block = h(31);
        assert_ne!(a.id_v1(), replay.id_v1(), "a raw claim re-admitted elsewhere is another source");
    }
    #[test]
    fn rfc40_envelope_preflight_rejects_lengths_truncation_trailing_and_nesting() {
        let (e, _, _) = contracts();
        let envelope = PalwPrefixDependencyEnvelopeV1 {
            inner: b"unchanged-original-payload".to_vec(),
            exports: vec![PalwSignedPrefixExportV1 {
                intent: e,
                signature: vec![0; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
            }],
            consumed: None,
        };
        let bytes = envelope.encode_v1().unwrap();
        assert_eq!(*PalwPrefixDependencyEnvelopeV1::decode_v1(&bytes).unwrap(), envelope);
        for n in [0, 3, 4, 7, bytes.len() - 1] {
            assert!(PalwPrefixDependencyEnvelopeV1::decode_v1(&bytes[..n]).is_err());
        }
        let mut malformed = bytes.clone();
        malformed[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(PalwPrefixDependencyEnvelopeV1::decode_v1(&malformed).is_err());
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(PalwPrefixDependencyEnvelopeV1::decode_v1(&trailing).is_err());
        let mut nested = envelope.clone();
        nested.inner = bytes;
        assert_eq!(nested.encode_v1(), Err(R::Contract));
        let mut duplicate = envelope.clone();
        duplicate.exports.push(duplicate.exports[0].clone());
        assert_eq!(duplicate.encode_v1(), Err(R::Contract));
        let mut bad_signature_shape = envelope;
        bad_signature_shape.exports[0].signature.pop();
        assert_eq!(bad_signature_shape.encode_v1(), Err(R::Signature));
    }
    #[test]
    fn rfc40_prefix_contract_real_signatures_bind_all_fields_without_acceptance_cycle() {
        let keys = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([71; 32]);
        let pk = keys.verification_key.as_ref();
        let (mut e, _, mut c) = contracts();
        e.producer_key_id = crate::mldsa87_primitives::mldsa87_key_id(pk);
        let sig =
            libcrux_ml_dsa::ml_dsa_87::sign(&keys.signing_key, e.id_v1().as_byte_slice(), PREFIX_EXPORT_CONTEXT_V1, [72; 32]).unwrap();
        e.verify_signature_v1(pk, sig.as_ref()).unwrap();
        let mut changed = e.clone();
        changed.material_until += 1;
        assert_eq!(changed.verify_signature_v1(pk, sig.as_ref()), Err(R::Signature));
        c.consumer_key_id = e.producer_key_id;
        let sig =
            libcrux_ml_dsa::ml_dsa_87::sign(&keys.signing_key, c.id_v1().as_byte_slice(), PREFIX_USE_CONTEXT_V1, [73; 32]).unwrap();
        c.verify_signature_v1(pk, sig.as_ref()).unwrap();
        c.source.accepted_block = h(44);
        assert_eq!(c.verify_signature_v1(pk, sig.as_ref()), Err(R::Signature));
    }
}

/// New outer FP provenance carriage, distinct from existing FPD1 (DSL).
/// Inner is the original transaction payload (plain/TFP1/HFP1/TFO1/FPA1),
/// never another FPR2. Its bytes and existing signature are not rewritten.
pub const PREFIX_DEPENDENCY_ENVELOPE_MAGIC_V1: &[u8; 4] = b"FPR2";

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwSignedPrefixExportV1 {
    pub intent: PalwPrefixExportIntentV1,
    pub signature: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwSignedPrefixUseV1 {
    pub intent: PalwPrefixUseIntentV1,
    pub signature: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPrefixDependencyEnvelopeV1 {
    pub inner: Vec<u8>,
    pub exports: Vec<PalwSignedPrefixExportV1>,
    pub consumed: Option<PalwSignedPrefixUseV1>,
}
impl PalwPrefixDependencyEnvelopeV1 {
    /// Allocation-free decoder view. Validates the complete typed wire shape;
    /// callers must still decode/authenticate provenance before claim admission.
    pub fn inner_view_v1(bytes: &[u8]) -> Result<&[u8], std::io::Error> {
        let bad = || std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid FPR2 view");
        let body = bytes.strip_prefix(PREFIX_DEPENDENCY_ENVELOPE_MAGIC_V1).ok_or_else(bad)?;
        crate::palw_native_resource_v1::preflight_native_value_v1::<Self>(body, body.len() as u64).map_err(|_| bad())?;
        let length = u32::from_le_bytes(body.get(..4).ok_or_else(bad)?.try_into().map_err(|_| bad())?) as usize;
        let inner = body.get(4..4usize.checked_add(length).ok_or_else(bad)?).ok_or_else(bad)?;
        if inner.is_empty() || inner.starts_with(PREFIX_DEPENDENCY_ENVELOPE_MAGIC_V1) {
            return Err(bad());
        }
        Ok(inner)
    }
    pub fn check_v1(&self) -> Result<(), R> {
        if self.inner.is_empty()
            || self.inner.starts_with(PREFIX_DEPENDENCY_ENVELOPE_MAGIC_V1)
            || (self.exports.is_empty() && self.consumed.is_none())
            || self.exports.windows(2).any(|x| x[0].intent.ordinal >= x[1].intent.ordinal)
        {
            return Err(R::Contract);
        }
        for e in &self.exports {
            e.intent.check_v1()?;
            if e.signature.len() != crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN {
                return Err(R::Signature);
            }
        }
        if let Some(c) = &self.consumed {
            c.intent.check_v1()?;
            if c.signature.len() != crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN {
                return Err(R::Signature);
            }
        }
        Ok(())
    }
    pub fn encode_v1(&self) -> Result<Vec<u8>, R> {
        self.check_v1()?;
        let mut out = PREFIX_DEPENDENCY_ENVELOPE_MAGIC_V1.to_vec();
        self.serialize(&mut out).map_err(|_| R::Contract)?;
        Ok(out)
    }
    /// Native preflight charges typed allocations before any untrusted Vec is
    /// constructed. Retain the returned owner through extraction/folding.
    pub fn decode_v1(
        bytes: &[u8],
    ) -> Result<
        crate::palw_native_resource_v1::NativeRetainedV1<Self>,
        crate::palw_native_resource_v1::NativeDecodeErrorV1<std::io::Error>,
    > {
        use crate::palw_native_resource_v1::NativeDecodeErrorV1;
        let bad =
            || NativeDecodeErrorV1::Decode(std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid prefix dependency envelope"));
        let body = bytes.strip_prefix(PREFIX_DEPENDENCY_ENVELOPE_MAGIC_V1).ok_or_else(bad)?;
        let value = super::decode_claim_local_value_v1::<Self>(body)?;
        value.check_v1().map_err(|_| bad())?;
        Ok(value)
    }
}
// Structural allocation and wire visitors are generated from these exact fields.

impl super::PalwClaimLocalAdmissionContextV1 {
    /// Resolve a source statement from its executed birth frame, not a caller's
    /// claimed accepting point. This authenticates provenance only; the court
    /// must separately bind the exported KV descriptor to the source execution.
    pub fn prefix_export_v1(&self, ordinal: u32) -> Result<Option<(&PalwSignedPrefixExportV1, PalwPrefixExportIdentityV1)>, R> {
        let mut envelopes = self.frame().inputs.iter().filter_map(|input| match input {
            super::PalwClaimLocalInputV1::PrefixDependency { claim, envelope } if *claim == self.claim_id() => Some(envelope.as_ref()),
            _ => None,
        });
        let Some(envelope) = envelopes.next() else { return Ok(None) };
        if envelopes.next().is_some() {
            return Err(R::Identity);
        }
        envelope.check_v1()?;
        let Some(export) = envelope.exports.iter().find(|e| e.intent.ordinal == ordinal) else { return Ok(None) };
        let mut jobs = self.frame().inputs.iter().filter_map(|input| match input {
            super::PalwClaimLocalInputV1::FreePromptNative { claim, public_job: Some(job), .. } if *claim == self.claim_id() => {
                Some(job.as_ref())
            }
            _ => None,
        });
        let Some(job) = jobs.next() else { return Err(R::Identity) };
        if jobs.next().is_some()
            || crate::palw_freeprompt_v3::fp_claim_id_v3(job) != self.claim_id()
            || export.intent.base_commitment != self.claim_id()
            || export.intent.network != job.job.network_domain
            || export.intent.prefix.class_id != self.original_claim().class_id
        {
            return Err(R::Identity);
        }
        if export.intent.material_until > self.original_claim().trace_retention_daa
            || export.intent.liability_until < self.accepting_point().daa_score
        {
            return Err(R::Retention);
        }
        export.intent.verify_signature_v1(&job.job.executor_pubkey, &export.signature)?;
        let point = self.accepting_point();
        Ok(Some((
            export,
            PalwPrefixExportIdentityV1 {
                intent: export.intent.id_v1(),
                source_claim: self.claim_id(),
                accepted_block: point.block,
                accepted_daa: point.daa_score,
                accepted_blue_score: point.blue_score,
            },
        )))
    }
}

/// Validate the same-carrier dependency against actual admitted source history.
/// No descriptor is credited here: missing replay/court binding remains held.
pub(crate) fn check_admission_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    params: &crate::palw_state_v2::PalwStateParamsV2,
    point: &crate::palw_state_v2::PalwBlockContextV2,
    object: &crate::palw_state_v2::PalwConsensusObjectV2,
    envelope: Option<&PalwPrefixDependencyEnvelopeV1>,
) -> Result<(), crate::palw_state_v2::PalwStateV2Error> {
    use crate::palw_state_v2::{PalwConsensusObjectV2, PalwStateV2Error};
    let bad = |why: R| PalwStateV2Error::CarriageInconsistent(format!("RFC0040: {why}"));
    if !params.claim_local_settlement_active_at(point.daa_score) {
        return if envelope.is_some() { Err(bad(R::Contract)) } else { Ok(()) };
    }
    let PalwConsensusObjectV2::FreePromptCommitted {
        claim,
        executor_pubkey,
        consumed_prefix_state,
        public_job,
        trace_retention_daa,
        class_id,
        ..
    } = object
    else {
        return if envelope.is_some() { Err(bad(R::Contract)) } else { Ok(()) };
    };
    let Some(envelope) = envelope else {
        return if consumed_prefix_state.is_genesis() { Ok(()) } else { Err(bad(R::Identity)) };
    };
    envelope.check_v1().map_err(bad)?;
    if !envelope.exports.is_empty() && !params.rcore_plus_active_at(point.daa_score) {
        return Err(bad(R::Contract));
    }
    let job = public_job.as_deref().ok_or_else(|| bad(R::Identity))?;
    if crate::palw_freeprompt_v3::fp_claim_id_v3(job) != *claim || job.job.executor_pubkey != *executor_pubkey {
        return Err(bad(R::Identity));
    }
    for export in &envelope.exports {
        if export.intent.base_commitment != *claim
            || export.intent.network != job.job.network_domain
            || export.intent.prefix.class_id != *class_id
        {
            return Err(bad(R::Identity));
        }
        if export.intent.liability_until < point.daa_score || export.intent.material_until > *trace_retention_daa {
            return Err(bad(R::Retention));
        }
        export.intent.verify_signature_v1(executor_pubkey, &export.signature).map_err(bad)?;
    }
    match (&envelope.consumed, consumed_prefix_state.is_genesis()) {
        (None, true) => Ok(()),
        (Some(consumed), false) => {
            if consumed.intent.prefix != *consumed_prefix_state {
                return Err(bad(R::Identity));
            }
            let mirror = params.claim_hard_deadline_v1().filter(|m| point.daa_score >= m.from_daa).ok_or_else(|| bad(R::Retention))?;
            let hard = if params.court_clock_v2_from_daa().is_some_and(|from| point.daa_score >= from) {
                point.daa_score.checked_add(crate::palw_court_policy_v1::DISPUTE_WINDOW_DAA).ok_or_else(|| bad(R::Retention))?
            } else {
                crate::palw_ruling_fences_v1::palw_x3_hard_deadline_at_v1(&mirror.policy, point.daa_score)
            };
            if consumed.intent.hard_deadline < hard || consumed.intent.material_until < *trace_retention_daa {
                return Err(bad(R::Retention));
            }
            // A re-export cannot outlive any transitive source. Admission of
            // each earlier source enforced the same bound at its own birth.
            if envelope.exports.iter().any(|e| {
                e.intent.liability_until > consumed.intent.hard_deadline || e.intent.material_until > consumed.intent.material_until
            }) {
                return Err(bad(R::Retention));
            }
            consumed.intent.verify_signature_v1(executor_pubkey, &consumed.signature).map_err(bad)?;
            let source = consumed.intent.source;
            let source_point = state.claim_local_prefix_source_point_v1(&source)?;
            let context =
                state.claim_local_admission_context_v1(params, &source_point, source.source_claim)?.ok_or_else(|| bad(R::Identity))?;
            // Find by the full intent id, never descriptor/hash coincidence.
            let export = context
                .frame()
                .inputs
                .iter()
                .filter_map(|input| match input {
                    super::PalwClaimLocalInputV1::PrefixDependency { claim, envelope } if *claim == source.source_claim => {
                        Some(envelope)
                    }
                    _ => None,
                })
                .flat_map(|envelope| envelope.exports.iter())
                .find(|e| e.intent.id_v1() == source.intent)
                .ok_or_else(|| bad(R::Identity))?;
            let (export, actual) = context.prefix_export_v1(export.intent.ordinal).map_err(bad)?.ok_or_else(|| bad(R::Identity))?;
            let reservation =
                state.prefix_export_reservation_v1(&source_point, source.source_claim)?.ok_or_else(|| bad(R::Retention))?;
            if reservation.convicted_daa.is_some() || reservation.liability_until < export.intent.liability_until || reservation.material_until < export.intent.material_until
            {
                return Err(bad(R::Retention));
            }
            consumed
                .intent
                .check_source_v1(&export.intent, &actual, job.job.network_domain, *claim, point.daa_score, point.blue_score)
                .map_err(bad)
        }
        _ => Err(bad(R::Contract)),
    }
}

impl super::PalwClaimLocalAdmissionContextV1 {
    /// Adjudicate an exported descriptor against the source execution's exact
    /// committed K/V openings. Signature validity never substitutes for this
    /// comparison; arithmetic correctness remains a separate step-court check.
    pub fn prefix_export_fault_v1(
        &self,
        ordinal: u32,
        binding: &crate::palw_step_leg::PalwStepBindingV2,
        tiles: &[crate::palw_prefix_state_court_v1::PalwPrefixKvTileV1],
        maximum_leaves: u64,
    ) -> Result<bool, &'static str> {
        let (export, _) = self
            .prefix_export_v1(ordinal)
            .map_err(|_| "unauthenticated source export")?
            .ok_or("source export absent from its actual accepting frame")?;
        let original = self.original_claim();
        if binding.committed_execution_root != original.execution_root || binding.full_logits_trace_root != original.trace_root {
            return Err("export opening belongs to another source execution");
        }
        let (job_pin, job) = self
            .frame()
            .inputs
            .iter()
            .find_map(|input| match input {
                super::PalwClaimLocalInputV1::FreePromptNative { claim, job_pin, public_job: Some(job), .. }
                    if *claim == self.claim_id() =>
                {
                    Some((*job_pin, job.as_ref()))
                }
                _ => None,
            })
            .ok_or("source public job absent")?;
        if crate::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(&binding.job_context) != job_pin {
            return Err("export binding changed the source job identity");
        }
        crate::palw_legacy_held_da_v2::palw_legacy_fp_job_check_v2(binding, &job.job)
            .map_err(|_| "export binding changed the source job")?;
        let root = crate::palw_prefix_state_court_v1::palw_prefix_kv_export_state_root_v1(
            binding,
            export.intent.prefix.prefix_tokens,
            tiles,
            maximum_leaves,
        )?;
        Ok(root != export.intent.prefix.state_root)
    }
}

#[path = "palw_claim_local_prefix_ingress_v1.rs"]
mod ingress;
pub use ingress::{PalwPrefixDependenciesV1, PalwPrefixExtrasV1};

/// Actual prospective collateral hold, rooted at source admission. Zero credit;
/// the native budget row carries the independently reserved liability.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPrefixExportReservationV1 {
    pub source_claim: Hash64,
    pub point: crate::palw_state_v2::PalwBlockContextV2,
    pub bond: crate::palw_state_v2::PalwBondKeyV2,
    pub budget_id: Hash64,
    pub liability_until: u64,
    pub material_until: u64,
    pub liability_sompi: u128,
    pub convicted_daa: Option<u64>,
}

/// Public source-export refutation. The source is an accepted identity, not a
/// claimed producer address or a matching cache hash. This envelope only carries
/// authenticated local cache openings; it is not a proof of model computation.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPrefixExportRefutationV1 {
    pub version: u8,
    pub source: PalwPrefixExportIdentityV1,
    pub ordinal: u32,
    pub binding: crate::palw_step_leg::PalwStepBindingV2,
    pub tiles: Vec<crate::palw_prefix_state_court_v1::PalwPrefixKvTileV1>,
}

impl crate::palw_state_v2::PalwChainStateV2 {
    /// Shared state-bound court predicate. The caller owns the bounded native
    /// decode ticket and charges the court work before invoking this predicate.
    /// A valid signature alone never returns a guilty finding.
    pub fn check_prefix_export_refutation_v1(
        &self,
        params: &crate::palw_state_v2::PalwStateParamsV2,
        point: &crate::palw_state_v2::PalwBlockContextV2,
        accused: &crate::palw_state_v2::PalwBondKeyV2,
        evidence: &PalwPrefixExportRefutationV1,
    ) -> Result<PalwPrefixExportReservationV1, crate::palw_state_v2::PalwStateV2Error> {
        use crate::palw_state_v2::PalwStateV2Error;
        let bad = |message: &'static str| PalwStateV2Error::CarriageInconsistent(message.into());
        if evidence.version != 1 || !params.claim_local_settlement_active_at(point.daa_score)
            || !params.claim_local_settlement_active_at(evidence.source.accepted_daa) {
            return Err(bad("prefix export court is dormant or has an unsupported version"));
        }
        let born = self.claim_local_prefix_source_point_v1(&evidence.source)?;
        let context = self.claim_local_admission_context_v1(params, &born, evidence.source.source_claim)?
            .ok_or_else(|| bad("prefix court has no actual source birth material"))?;
        if context.original_claim().bond != *accused {
            return Err(bad("prefix court accused another producer"));
        }
        let (export, identity) = context.prefix_export_v1(evidence.ordinal)
            .map_err(|_| bad("prefix court source signature is invalid"))?
            .ok_or_else(|| bad("prefix court export was not accepted"))?;
        if identity != evidence.source || point.daa_score > export.intent.liability_until
            || point.blue_score <= born.blue_score {
            return Err(bad("prefix court identity or signed liability window differs"));
        }
        let reservation = self.prefix_export_reservation_v1(&born, evidence.source.source_claim)?
            .ok_or_else(|| bad("prefix court source has no collateral reservation"))?;
        if reservation.bond != *accused || reservation.liability_until < export.intent.liability_until
            || reservation.material_until < export.intent.material_until {
            return Err(bad("prefix court source is not backed by its original reservation"));
        }
        if !context.prefix_export_fault_v1(evidence.ordinal, &evidence.binding, &evidence.tiles,
            crate::palw_context_ladder::PALW_CONTEXT_LADDER_MAX_STEP_LEAVES)
            .map_err(bad)? {
            return Err(bad("prefix export agrees with the authenticated source cache"));
        }
        Ok(reservation)
    }
}

/// One producer conviction per accepted source cohort, independently of which
/// export ordinal first exposed the lie. Re-admission has a different key.
pub fn prefix_export_offence_id_v1(
    accused: &crate::tx::TransactionOutpoint,
    source: &PalwPrefixExportIdentityV1,
) -> Hash64 {
    Hash64::from_bytes(misaka_palw_kernel::hash::object_id(
        b"misaka-palw/prefix-export/conviction/v1",
        &(accused, source.source_claim, source.accepted_block, source.accepted_daa, source.accepted_blue_score),
    ))
}
