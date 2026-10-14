//! Prospective native custody session and authenticated capture, independent of
//! the final carrying header hash. PAV2 remains an unchanged historical codec.
use crate::{
    Hash64,
    palw_attempt_v2::{PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2, execution_commitment_v3},
    palw_material_capacity_v1::{PalwMaterialCodecV1, PalwMaterialManifestV1},
    palw_state_v2::{PalwBondKeyV2, PalwChainStateV2},
};
use borsh::{BorshDeserialize, BorshSerialize};
pub const PALW_ATTEMPT_MATERIAL_WIRE_MAGIC_V3: [u8; 4] = *b"PAV3";
pub const PALW_ATTEMPT_MATERIAL_MAX_BYTES_V3: usize = 65536;
pub const PALW_MATERIAL_READY_CONTEXT_V3: &[u8] = b"MISAKA/PALW/MATERIAL-READY/V3";
/// The executor's seal over the complete Ready plus every provider signature.
pub const PALW_MATERIAL_SEAL_CONTEXT_V3: &[u8] = b"MISAKA/PALW/MATERIAL-SEAL/V3";
/// At most this many providers, so the carriage including the executor key/signature, every
/// provider signature and the seal stays inside [`PALW_ATTEMPT_MATERIAL_MAX_BYTES_V3`].
pub const PALW_MATERIAL_MAX_PROVIDERS_V3: usize = 8;
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwNativeMaterialSessionV3 {
    pub version: u16,
    pub class_id: Hash64,
    pub artifact_root: Hash64,
    pub executor: PalwBondKeyV2,
    pub operator: Hash64,
    /// Consensus-derived anchor/bucket policy, never a freely substituted job.
    pub job_policy: Hash64,
    pub codec: PalwMaterialCodecV1,
    pub max_units: u32,
    pub one_claim_cap: u8,
    pub admit_from_daa: u64,
    pub admit_until_daa: u64,
}
pub fn native_job_policy_v3() -> Hash64 {
    hash(b"MISAKA/PALW/NATIVE-JOB-POLICY/V3", &(3u16, crate::palw_attempt_v2::PALW_ATTEMPT_V2_TRACE_CHUNKS))
}
impl PalwNativeMaterialSessionV3 {
    pub fn matches(&self, a: &PalwAttemptUnsignedV2, daa: u64) -> bool {
        self.version == 3
            && self.class_id == a.class_id
            && self.artifact_root == a.artifact_root
            && self.executor.0 == a.executor_bond
            && self.operator == a.operator_id
            && self.job_policy == native_job_policy_v3()
            && self.one_claim_cap == 1
            && self.max_units > 0
            && self.max_units as usize <= crate::palw_material_capacity_v1::PALW_MATERIAL_MAX_UNITS_V1
            && self.admit_from_daa <= daa
            && daa <= self.admit_until_daa
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwNativeMaterialBindingV3 {
    pub claim: Hash64,
    pub execution_key: Hash64,
    pub manifest: PalwMaterialManifestV1,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwNativeMaterialReadyV3 {
    pub version: u16,
    pub session: Hash64,
    pub consent: Hash64,
    pub binding: PalwNativeMaterialBindingV3,
    pub providers: Vec<PalwBondKeyV2>,
}
impl PalwNativeMaterialReadyV3 {
    pub fn id(&self) -> Hash64 {
        hash(b"MISAKA/PALW/NATIVE-MATERIAL-READY/V3", self)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwAttemptMaterialEnvelopeV3 {
    pub version: u16,
    pub base: PalwAttemptEnvelopeV2,
    pub ready: PalwNativeMaterialReadyV3,
    /// In exactly the prior consent's provider order. Auth, never entropy.
    pub provider_signatures: Vec<Vec<u8>>,
    /// The executor's ML-DSA-87 signature over [`Self::seal_message`]. Provider signatures are
    /// outside the Layer-0 tag (they are not lottery entropy) but inside the block identity, so
    /// without a stateless seal any relayer could flip a provider-signature byte and mint a second
    /// header from one solve before the stateful roster check ran (the launch-blockers §5 class
    /// the PAV2 executor signature is checked at the header stage for). The seal is verified at
    /// the header stage with the attempt's own carried key; whether that key and every provider
    /// key are the registered bonds' remains the stateful admission's question.
    pub seal: Vec<u8>,
}
fn hash<T: BorshSerialize>(d: &[u8], x: &T) -> Hash64 {
    Hash64::from_bytes(misaka_palw_kernel::hash::object_id(d, x))
}
impl PalwAttemptMaterialEnvelopeV3 {
    /// Producer-side assembly: the exact signed base attempt, the Ready every provider signed
    /// (from [`PalwNativeMaterialReadyV3::id`], after each provider atomically published its
    /// complete typed bundle) and the executor's seal over both. Shape is checked before the seal
    /// is requested, so a malformed roster never obtains an executor signature.
    pub fn assemble(
        base: PalwAttemptEnvelopeV2,
        ready: PalwNativeMaterialReadyV3,
        provider_signatures: Vec<Vec<u8>>,
        seal_with: impl FnOnce(&[u8], &[u8]) -> Result<Vec<u8>, String>,
    ) -> Result<Self, String> {
        let mut out =
            Self { version: 3, base, ready, provider_signatures, seal: vec![0; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN] };
        out.validate_shape()?;
        out.seal = seal_with(out.seal_message().as_byte_slice(), PALW_MATERIAL_SEAL_CONTEXT_V3)?;
        out.validate_shape()?;
        if out.encode_wire().len() > PALW_ATTEMPT_MATERIAL_MAX_BYTES_V3 {
            return Err("native ready carriage exceeds cap".into());
        }
        Ok(out)
    }
    pub fn is_carriage(bytes: &[u8]) -> bool {
        bytes.starts_with(&PALW_ATTEMPT_MATERIAL_WIRE_MAGIC_V3)
    }
    pub fn encode_wire(&self) -> Vec<u8> {
        let mut out = PALW_ATTEMPT_MATERIAL_WIRE_MAGIC_V3.to_vec();
        out.extend(borsh::to_vec(self).expect("native ready encodes"));
        out
    }
    pub fn decode_wire(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > PALW_ATTEMPT_MATERIAL_MAX_BYTES_V3 {
            return Err("native ready carriage exceeds cap".into());
        }
        let body = bytes.strip_prefix(&PALW_ATTEMPT_MATERIAL_WIRE_MAGIC_V3).ok_or("native ready carriage magic")?;
        let out: Self = borsh::from_slice(body).map_err(|e| e.to_string())?;
        out.validate_shape()?;
        Ok(out)
    }
    pub fn validate_shape(&self) -> Result<(), String> {
        let r = &self.ready;
        if self.version != 3
            || r.version != 3
            || r.session == Hash64::default()
            || r.consent == Hash64::default()
            || r.providers.len() < 2
            || r.providers.len() > PALW_MATERIAL_MAX_PROVIDERS_V3
            || r.providers.windows(2).any(|v| v[0] >= v[1])
            || self.provider_signatures.len() != r.providers.len()
            || self.provider_signatures.iter().any(|s| s.len() != crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN)
            || self.seal.len() != crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN
            || r.binding.claim != attempt_id_v2(&self.base.attempt)
            || r.binding.manifest.root != r.session
            || r.binding.manifest.scopes.len() != 1
        {
            return Err("native ready version, roster, capacity or exact attempt identity".into());
        }
        let scope = &r.binding.manifest.scopes[0];
        if scope.claim != r.binding.claim || scope.commitment != self.base.attempt.execution_root {
            return Err("native ready manifest substitutes immutable execution".into());
        }
        Ok(())
    }
    /// The executor-sealed message: the exact Ready identity and every provider signature in
    /// roster order. Not priced: re-sealing buys no ticket.
    pub fn seal_message(&self) -> Hash64 {
        hash(
            b"MISAKA/PALW/NATIVE-MATERIAL-SEAL/V3",
            &(self.version, self.ready.id(), attempt_id_v2(&self.base.attempt), &self.provider_signatures),
        )
    }
    /// Stateless header-stage check: shape plus the executor seal under the attempt's own key.
    pub fn verify_seal(&self, verify: impl Fn(&[u8], &[u8], &[u8], &[u8]) -> bool) -> Result<(), String> {
        self.validate_shape()?;
        if !verify(&self.base.attempt.executor_pubkey, self.seal_message().as_byte_slice(), &self.seal, PALW_MATERIAL_SEAL_CONTEXT_V3)
        {
            return Err("native ready executor seal invalid".into());
        }
        Ok(())
    }
    /// Deterministic price projection: content digest and measured bytes are
    /// provider-authenticated custody facts, not selectable work entropy. The
    /// layout is derived from the actual immutable attempt and fixed prior roster.
    pub fn priced_execution(&self, anchor: Hash64) -> Hash64 {
        hash(
            b"MISAKA/PALW/NATIVE-MATERIAL-EXECUTION/V3",
            &(
                execution_commitment_v3(&self.base.attempt, anchor),
                self.ready.session,
                self.ready.consent,
                &self.ready.providers,
                self.base.attempt.execution_root,
                self.base.attempt.trace_root,
                self.base.attempt.trace_chunk_count,
            ),
        )
    }
    pub fn class_ticket(&self, anchor: Hash64) -> u128 {
        let digest = hash(b"MISAKA/PALW/NATIVE-MATERIAL-CLASS-TICKET/V3", &self.priced_execution(anchor));
        u128::from_le_bytes(digest.as_byte_slice()[..16].try_into().unwrap())
    }
    pub fn verify_prior(
        &self,
        state: &PalwChainStateV2,
        anchor: Hash64,
        daa: u64,
        verify: impl Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
    ) -> Result<(), String> {
        self.validate_shape()?;
        let a = &self.base.attempt;
        // The canonical oldest-key eligible session fixes the roster before work;
        // choosing another funded session cannot buy another draw for that work.
        let selected = state
            .monetary_material_rows_v1()
            .into_iter()
            .filter(|row| {
                row.native_binding.is_none()
                    && row.terms.native_session.as_ref().is_some_and(|s| s.matches(a, daa))
                    && row.healthy(state)
            })
            .min_by_key(|row| row.terms.root)
            .ok_or("no eligible prior signed native session")?;
        if selected.terms.root != self.ready.session
            || selected.consent != self.ready.consent
            || selected.providers.iter().filter(|p| !p.charged).map(|p| p.provider).collect::<Vec<_>>() != self.ready.providers
        {
            return Err("native ready substitutes prior session/provider roster".into());
        }
        let session = selected.terms.native_session.as_ref().unwrap();
        let scope = &self.ready.binding.manifest.scopes[0];
        if scope.codec != session.codec
            || scope.units > session.max_units
            || self.ready.binding.execution_key != execution_commitment_v3(a, anchor)
            || selected.terms.material_manifest.is_some()
        {
            return Err("native ready codec, actual execution or units differ from signed session".into());
        }
        self.ready.binding.manifest.validate(selected.terms.max_material_bytes).map_err(|e| e.to_string())?;
        let id = self.ready.id();
        if !verify(&a.executor_pubkey, self.seal_message().as_byte_slice(), &self.seal, PALW_MATERIAL_SEAL_CONTEXT_V3) {
            return Err("native ready executor seal invalid".into());
        }
        for (provider, sig) in self.ready.providers.iter().zip(&self.provider_signatures) {
            let bond = state.bond(provider).ok_or("ready provider bond absent")?;
            if !verify(&bond.pubkey, id.as_byte_slice(), sig, PALW_MATERIAL_READY_CONTEXT_V3) {
                return Err("native material provider ready signature invalid".into());
            }
        }
        Ok(())
    }
}
/// Explicit parser for the two native header forms. Old decode_wire accepts
/// only PAV2; owned-form callers use this function after their fence check.
pub fn native_attempt_base_v3(bytes: &[u8]) -> Result<PalwAttemptEnvelopeV2, String> {
    if PalwAttemptMaterialEnvelopeV3::is_carriage(bytes) {
        Ok(PalwAttemptMaterialEnvelopeV3::decode_wire(bytes)?.base)
    } else {
        PalwAttemptEnvelopeV2::decode_wire(bytes).map_err(|e| e.to_string())
    }
}

pub fn verify_ready_signature_v3(key: &[u8], message: &[u8], sig: &[u8], context: &[u8]) -> bool {
    let Ok(key): Result<[u8; crate::mldsa87_primitives::MLDSA87_PUBKEY_LEN], _> = key.try_into() else { return false };
    let Ok(sig): Result<[u8; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN], _> = sig.try_into() else { return false };
    let key = libcrux_ml_dsa::ml_dsa_87::MLDSA87VerificationKey::new(key);
    let sig = libcrux_ml_dsa::ml_dsa_87::MLDSA87Signature::new(sig);
    libcrux_ml_dsa::ml_dsa_87::portable::verify(&key, message, context, &sig).is_ok()
}

#[derive(Clone, Debug)]
pub struct PalwNativeMaterialProviderAuthorityV3 {
    pub row: crate::palw_monetary_material_v1::PalwMonetaryMaterialRowV1,
    pub material: crate::palw_material_capacity_v1::PalwMaterialNativeAuthorityV1,
    pub provider_keys: Vec<(PalwBondKeyV2, Vec<u8>)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The seal message moves with every provider signature byte and with the Ready identity, so
    /// a relayed byte flip in a provider signature cannot keep a valid seal (header-stage
    /// malleability), while nothing in it is priced (re-sealing buys no Layer-0 ticket).
    #[test]
    fn seal_covers_provider_signatures_and_is_not_priced() {
        let sig = vec![7u8; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN];
        let base = crate::palw_attempt_v2::tests::envelope(crate::palw_attempt_v2::tests::attempt());
        let claim = attempt_id_v2(&base.attempt);
        let manifest = PalwMaterialManifestV1 {
            root: Hash64::from_u64_word(1),
            scopes: vec![crate::palw_material_capacity_v1::PalwMaterialScopeV1 {
                claim,
                commitment: base.attempt.execution_root,
                codec: PalwMaterialCodecV1::NativeHeldComplete,
                bytes: 1,
                units: 1,
                content: Hash64::from_u64_word(6),
            }],
        };
        let bond = |word| PalwBondKeyV2(crate::tx::TransactionOutpoint::new(Hash64::from_u64_word(word), 0));
        let ready = PalwNativeMaterialReadyV3 {
            version: 3,
            session: manifest.root,
            consent: Hash64::from_u64_word(2),
            binding: PalwNativeMaterialBindingV3 { claim, execution_key: Hash64::from_u64_word(3), manifest },
            providers: vec![bond(4), bond(5)],
        };
        let envelope = PalwAttemptMaterialEnvelopeV3::assemble(base, ready, vec![sig.clone(), sig.clone()], |_, context| {
            assert_eq!(context, PALW_MATERIAL_SEAL_CONTEXT_V3);
            Ok(sig.clone())
        })
        .unwrap();
        let anchor = Hash64::from_u64_word(5);
        let mut flipped = envelope.clone();
        flipped.provider_signatures[1][0] ^= 1;
        assert_ne!(flipped.seal_message(), envelope.seal_message());
        assert_eq!(flipped.priced_execution(anchor), envelope.priced_execution(anchor));
        let mut resealed = envelope.clone();
        resealed.seal[0] ^= 1;
        assert_eq!(resealed.class_ticket(anchor), envelope.class_ticket(anchor));
        assert_eq!(PalwAttemptMaterialEnvelopeV3::decode_wire(&envelope.encode_wire()).unwrap(), envelope);
        assert!(envelope.verify_seal(|_, _, _, _| false).is_err());
    }
}
