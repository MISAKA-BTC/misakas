//! Private RFC-0029 candidate: literal producer signature and named numerical
//! root tied to the retained public original/source/profile, plus private native
//! canonical retention. No public lifecycle decoder/dispatcher, paid admission,
//! notice or default right.
use crate::palw_court_close_receipt_v1::NativeCourtRulesV1;
use crate::palw_court_shared_tir_stream_v1::stream_digest;
use crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1 as Key;
use crate::palw_tir_dissect_v1::{PalwTirDissectSiteV1, PalwTirRootClaimV1};
use crate::{
    Hash64,
    palw_state_v2::{PalwBondKeyV2, PalwBondStatusV2, PalwChainStateV2, PalwStateParamsV2},
};
use borsh::{BorshDeserialize, BorshSerialize};

pub const SHARED_TIR_ROOT_CONTEXT_V1: &[u8] = b"misaka-palw/court-shared-tir-root/v1/mldsa87";

/// Native caller supplies actual validation config, never wire-supplied rules.
/// Excludes ONLY the accepting point: producers cannot sign a future block.
/// This is a validation-policy identity, not a fee/capacity/adoption certificate.
pub fn shared_tir_root_policy_id_v1(r: &NativeCourtRulesV1) -> Result<Hash64, String> {
    r.check()?;
    if !r.tir_active || !crate::palw_attn_dissect::palw_attn_arity_is_legal_v1(r.court.dissection_arity()) {
        return Err("shared numerical root needs active TIR and legal native arity".into());
    }
    stream_digest(
        b"misaka-palw/shared-tir-root-policy/v1",
        &(
            r.network_domain,
            r.court,
            r.step_ladder,
            r.prompt_ids_merkle,
            r.held_regime,
            r.decode_close_convicts_only,
            r.fp_decode_from_daa,
            r.cell_closure_from_daa,
            r.memory_routing_from_daa,
            r.tir_active,
            r.gen_active,
            r.task_heads_active,
            r.improvement_active,
            r.constraint_active,
        ),
    )
}

/// In-memory DTO only. No unbounded derived wire deserializer or lifecycle tag.
/// No owner, requester or relay contributes to this fixed root's identity.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct SignedSharedTirRootV1 {
    pub version: u16,
    pub claim: Hash64,
    pub scope_id: Hash64,
    pub policy_id: Hash64,
    pub leaf: u64,
    pub arity: u8,
    pub signer: PalwBondKeyV2,
    pub root: Box<PalwTirRootClaimV1>,
    pub signature: Vec<u8>,
}
/// Existing legacy root carrier overhead, counted from its actual field
/// grammar (one-byte lifecycle tag, session, root body, arity, signature).
/// The extra signer reference is the existing admission allowance. This is a
/// width exemplar only, never a lifecycle message or signature assertion.
fn legacy_root_overhead_v1() -> Result<u64, String> {
    let fields = (0u8, Hash64::default(), (), 0u8, Vec::<u8>::new());
    (borsh::object_length(&fields).map_err(|e| e.to_string())? as u64)
        .checked_add(crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN as u64)
        .and_then(|n| n.checked_add(crate::palw_tir_close_size_v1::MOVE_CARRIER_EXTRA_BYTES))
        .ok_or_else(|| "shared root carrier count overflow".into())
}
/// Existing root payload allowance; new shared header/transport is additional.
pub fn shared_root_body_ceiling_v1() -> Result<u64, String> {
    crate::palw_tir_admission_v1::PALW_TIR_DISSECT_CARRIER_BYTES_V1
        .checked_sub(legacy_root_overhead_v1()?)
        .ok_or_else(|| "shared root carrier allowance underflow".into())
}
pub fn shared_root_carried_bytes_v1(root: &PalwTirRootClaimV1) -> Result<u64, String> {
    (borsh::object_length(root).map_err(|e| e.to_string())? as u64)
        .checked_add(legacy_root_overhead_v1()?)
        .ok_or_else(|| "shared root carrier count overflow".into())
}
/// Same old payload/carrier allowance, no buffer/clone, cap increase, or claim
/// that the larger new shared envelope fits that single carrier.
pub fn check_shared_root_cost_v1(root: &PalwTirRootClaimV1) -> Result<(), String> {
    if shared_root_carried_bytes_v1(root)? > crate::palw_tir_admission_v1::PALW_TIR_DISSECT_CARRIER_BYTES_V1 {
        return Err("shared root exceeds existing root carrier allowance".into());
    }
    Ok(())
}
impl SignedSharedTirRootV1 {
    pub fn signing_message(&self, network: Hash64) -> Result<Hash64, String> {
        if self.version != 1 {
            return Err("shared TIR root version".into());
        }
        stream_digest(
            b"misaka-palw/shared-tir-root-intent/v1",
            &(
                network,
                self.version,
                self.claim,
                self.scope_id,
                self.policy_id,
                self.leaf,
                self.arity,
                self.signer,
                self.root.as_ref(),
            ),
        )
    }
}

/// Cannot be deserialized or constructed from caller-supplied `checked=true`.
/// A successful result is a producer-authenticated numerical statement, which
/// MAY be false and require bottom adjudication. It is not computation credit.
#[derive(Debug)]
pub struct CheckedSharedTirRootV1<'a> {
    control: &'a SignedSharedTirRootV1,
    site: PalwTirDissectSiteV1,
}
impl<'a> CheckedSharedTirRootV1<'a> {
    pub fn control(&self) -> &'a SignedSharedTirRootV1 {
        self.control
    }
    pub fn site(&self) -> &PalwTirDissectSiteV1 {
        &self.site
    }
}

/// Caller must admit signature/hash/source/finalize/clone work and provide the
/// actual native params/rules. No mutation, reservation or accepting permission
/// follows; canonical root uniqueness and full funded ingress remain separate.
pub fn check_shared_tir_root_v1<'a>(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    rules: &NativeCourtRulesV1,
    control: &'a SignedSharedTirRootV1,
) -> Result<CheckedSharedTirRootV1<'a>, String> {
    let bond = state.bond(&control.signer).ok_or("shared TIR root producer bond missing")?;
    if !matches!(bond.status, PalwBondStatusV2::Active) || bond.collateral < params.min_collateral_sompi() {
        return Err("shared TIR root producer not active/above existing floor".into());
    }
    check_shared_tir_root_statement_v1(state, rules, control, &bond.pubkey)
}

/// Historical verification uses the key authenticated at acceptance. Later exit
/// or key change must not invalidate retained public numerical material.
fn check_shared_tir_root_statement_v1<'a>(
    state: &PalwChainStateV2,
    rules: &NativeCourtRulesV1,
    control: &'a SignedSharedTirRootV1,
    pubkey: &[u8],
) -> Result<CheckedSharedTirRootV1<'a>, String> {
    if control.signature.len() != crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN {
        return Err("shared TIR root signature shape".into());
    }
    check_shared_root_cost_v1(&control.root)?;
    let policy = shared_tir_root_policy_id_v1(rules)?;
    let c = crate::palw_court_shared_tir_state_v1::context(state, control.claim)?;
    let source = c.source(state)?;
    if control.scope_id != c.scope_id
        || control.policy_id != policy
        || control.arity != rules.court.dissection_arity()
        || rules.at.daa_score < c.original_accepted_daa
        || rules.at.daa_score >= c.hard_daa
    {
        return Err("shared TIR root changes original scope/policy/arity or hard interval".into());
    }
    let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, control.claim)
        .ok_or("shared TIR root has no public original")?;
    let claim = state.claim(&control.claim).ok_or("shared TIR root has no retained claim")?;
    if control.signer != frame.custody_v1().pledge_v1().producer
        || control.signer != claim.bond
        || crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1::of_binding_v1(&control.root.finalize.binding)
            != *source.header_v1()
    {
        return Err("shared TIR root substitutes literal producer or original header".into());
    }
    if !crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1(
        pubkey,
        control.signing_message(rules.network_domain)?.as_byte_slice(),
        &control.signature,
        SHARED_TIR_ROOT_CONTEXT_V1,
    ) {
        return Err("shared TIR root producer signature invalid".into());
    }
    let prompt = if rules.prompt_ids_merkle {
        crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1
    } else {
        crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat
    };
    let site = crate::palw_court_v2::check_named_tir_root_for_claim_v1(
        state,
        &control.claim,
        control.leaf,
        &control.root,
        control.arity,
        rules.court.dissection_arity(),
        &rules.court,
        rules.step_ladder,
        prompt,
    )
    .map_err(|e| e.to_string())?;
    Ok(CheckedSharedTirRootV1 { control, site })
}

/// Private native snapshot material, not a public lifecycle DTO. Native carriage
/// preflight scans the complete nested shape before this row is deserialized.
/// Retains the first signed root and accepting rules; duplicate legal finalize
/// spellings with the same numerical root do not allocate another root.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct SharedTirRootRowV1 {
    pub version: u16,
    pub control: SignedSharedTirRootV1,
    pub rules: NativeCourtRulesV1,
    pub signer_pubkey: Vec<u8>,
    pub material_until_daa: u64,
}
impl BorshDeserialize for SharedTirRootRowV1 {
    fn deserialize_reader<R: std::io::Read>(r: &mut R) -> std::io::Result<Self> {
        let version = u16::deserialize_reader(r)?;
        if version != 1 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "shared root row version"));
        }
        // This is a native snapshot row only. Its enclosing native reader has
        // already scanned root vectors/boxes and reserved their actual shape.
        let control = SignedSharedTirRootV1 {
            version: u16::deserialize_reader(r)?,
            claim: Hash64::deserialize_reader(r)?,
            scope_id: Hash64::deserialize_reader(r)?,
            policy_id: Hash64::deserialize_reader(r)?,
            leaf: u64::deserialize_reader(r)?,
            arity: u8::deserialize_reader(r)?,
            signer: PalwBondKeyV2::deserialize_reader(r)?,
            root: Box::deserialize_reader(r)?,
            signature: crate::palw_public_binding_v1::read_signature_v1(r)?,
        };
        let rules = NativeCourtRulesV1::deserialize_reader(r)?;
        let signer_pubkey = crate::palw_public_binding_v1::read_bounded_bytes_v1(r, crate::mldsa87_primitives::MLDSA87_PUBKEY_LEN)?;
        if signer_pubkey.len() != crate::mldsa87_primitives::MLDSA87_PUBKEY_LEN {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "shared root row public key length"));
        }
        let material_until_daa = u64::deserialize_reader(r)?;
        Ok(Self { version, control, rules, signer_pubkey, material_until_daa })
    }
}
impl SharedTirRootRowV1 {
    pub fn check(&self, key: &Key) -> Result<(), String> {
        if self.version != 1
            || self.control.version != 1
            || *key != Key::CourtSharedRoot(self.control.claim, self.control.leaf)
            || self.control.policy_id != shared_tir_root_policy_id_v1(&self.rules)?
            || self.control.arity != self.rules.court.dissection_arity()
            || self.signer_pubkey.len() != crate::mldsa87_primitives::MLDSA87_PUBKEY_LEN
            || self.control.signature.len() != crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN
        {
            return Err("invalid canonical shared root key/version/policy/signature shape".into());
        }
        Ok(())
    }
    fn checked_statement(&self, state: &PalwChainStateV2) -> Result<CheckedSharedTirRootV1<'_>, String> {
        self.check(&Key::CourtSharedRoot(self.control.claim, self.control.leaf))?;
        let c = crate::palw_court_shared_tir_state_v1::context(state, self.control.claim)?;
        if self.material_until_daa != c.material_until_daa {
            return Err("shared root changes original material horizon".into());
        }
        check_shared_tir_root_statement_v1(state, &self.rules, &self.control, &self.signer_pubkey)
    }
    pub fn check_context(&self, state: &PalwChainStateV2) -> Result<(), String> {
        self.checked_statement(state).map(|_| ())
    }
    /// Arithmetic view only; never install its projected phases or interpret
    /// historic phase clocks as an accepted shared response/default deadline.
    pub fn numerical_frontier(
        &self,
        state: &PalwChainStateV2,
    ) -> Result<crate::palw_tir_dissect_v1::PalwTirNumericalFrontierV1, String> {
        let checked = self.checked_statement(state)?;
        let id = stream_digest(
            b"misaka-palw/shared-tir-numerical-slot/v1",
            &(self.control.claim, self.control.scope_id, self.control.leaf),
        )?;
        let phase = crate::palw_tir_dissect_v1::PalwTirDissectPhaseV1::open(
            id,
            self.control.leaf,
            checked.site(),
            &self.control.root,
            self.control.arity,
            self.rules.at.daa_score,
            self.rules.court.turn_deadline_daa(),
        )
        .map_err(|e| e.to_string())?;
        crate::palw_tir_dissect_v1::PalwTirNumericalFrontierV1::new(phase)
    }
    pub fn check_loaded_context(&self, state: &PalwChainStateV2) -> Result<(), String> {
        self.check_context(state)?;
        if state.last_point().is_none_or(|p| p.daa_score < self.rules.at.daa_score) {
            return Err("shared root lies beyond accepted snapshot".into());
        }
        Ok(())
    }
    /// A second already-validated carriage may have other legitimate finalize
    /// openings/signature bytes. Only the fixed numerical statement coalesces;
    /// a changed policy/arity/root cannot replace or create a parallel slot.
    pub fn same_numerical_root(&self, other: &SignedSharedTirRootV1) -> bool {
        let a = &self.control;
        (a.version, a.claim, a.scope_id, a.policy_id, a.leaf, a.arity, a.signer)
            == (other.version, other.claim, other.scope_id, other.policy_id, other.leaf, other.arity, other.signer)
            && a.root.version == other.root.version
            && a.root.elements == other.root.elements
            && a.root.totals == other.root.totals
    }
}
