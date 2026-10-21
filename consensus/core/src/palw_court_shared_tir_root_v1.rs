//! Private RFC-0029 candidate: literal producer signature and named numerical
//! root tied to the retained public original/source/profile. Read-only, no wire
//! dispatcher/decoder, rooted uniqueness, paid admission, notice or default right.
use crate::{Hash64, palw_state_v2::{PalwBondKeyV2, PalwChainStateV2, PalwStateParamsV2, PalwBondStatusV2}};
use crate::palw_court_close_receipt_v1::NativeCourtRulesV1;
use crate::palw_tir_dissect_v1::{PalwTirRootClaimV1, PalwTirDissectSiteV1};
use crate::palw_court_shared_tir_stream_v1::stream_digest;
use borsh::BorshSerialize;

pub const SHARED_TIR_ROOT_CONTEXT_V1: &[u8] = b"misaka-palw/court-shared-tir-root/v1/mldsa87";

/// Native caller supplies actual validation config, never wire-supplied rules.
/// Excludes ONLY the accepting point: producers cannot sign a future block.
/// This is a validation-policy identity, not a fee/capacity/adoption certificate.
pub fn shared_tir_root_policy_id_v1(r: &NativeCourtRulesV1) -> Result<Hash64, String> {
    r.check()?;
    if !r.tir_active || !crate::palw_attn_dissect::palw_attn_arity_is_legal_v1(r.court.dissection_arity()) {
        return Err("shared numerical root needs active TIR and legal native arity".into());
    }
    stream_digest(b"misaka-palw/shared-tir-root-policy/v1", &(r.network_domain, r.court, r.step_ladder,
        r.prompt_ids_merkle, r.held_regime, r.decode_close_convicts_only, r.fp_decode_from_daa,
        r.cell_closure_from_daa, r.memory_routing_from_daa, r.tir_active, r.gen_active,
        r.task_heads_active, r.improvement_active, r.constraint_active))
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
impl SignedSharedTirRootV1 {
    pub fn signing_message(&self, network: Hash64) -> Result<Hash64, String> {
        if self.version != 1 { return Err("shared TIR root version".into()); }
        stream_digest(b"misaka-palw/shared-tir-root-intent/v1", &(network, self.version, self.claim,
            self.scope_id, self.policy_id, self.leaf, self.arity, self.signer, self.root.as_ref()))
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
    pub fn control(&self) -> &'a SignedSharedTirRootV1 { self.control }
    pub fn site(&self) -> &PalwTirDissectSiteV1 { &self.site }
}

/// Caller must admit signature/hash/source/finalize/clone work and provide the
/// actual native params/rules. No mutation, reservation or accepting permission
/// follows; canonical root uniqueness and full funded ingress remain separate.
pub fn check_shared_tir_root_v1<'a>(state: &PalwChainStateV2, params: &PalwStateParamsV2,
    rules: &NativeCourtRulesV1, control: &'a SignedSharedTirRootV1) -> Result<CheckedSharedTirRootV1<'a>, String> {
    if control.signature.len() != crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN {
        return Err("shared TIR root signature shape".into());
    }
    let policy = shared_tir_root_policy_id_v1(rules)?;
    let c = crate::palw_court_shared_tir_state_v1::context(state, control.claim)?;
    let source = c.source(state)?;
    if control.scope_id != c.scope_id || control.policy_id != policy
        || control.arity != rules.court.dissection_arity()
        || rules.at.daa_score < c.original_accepted_daa || rules.at.daa_score >= c.hard_daa {
        return Err("shared TIR root changes original scope/policy/arity or hard interval".into());
    }
    let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, control.claim)
        .ok_or("shared TIR root has no public original")?;
    let claim = state.claim(&control.claim).ok_or("shared TIR root has no retained claim")?;
    if control.signer != frame.custody_v1().pledge_v1().producer || control.signer != claim.bond
        || crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1::of_binding_v1(&control.root.finalize.binding)
            != *source.header_v1() {
        return Err("shared TIR root substitutes literal producer or original header".into());
    }
    let bond = state.bond(&control.signer).ok_or("shared TIR root producer bond missing")?;
    if !matches!(bond.status, PalwBondStatusV2::Active) || bond.collateral < params.min_collateral_sompi() {
        return Err("shared TIR root producer not active/above existing floor".into());
    }
    if !crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1(&bond.pubkey,
        control.signing_message(rules.network_domain)?.as_byte_slice(), &control.signature, SHARED_TIR_ROOT_CONTEXT_V1) {
        return Err("shared TIR root producer signature invalid".into());
    }
    let prompt = if rules.prompt_ids_merkle { crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1 }
        else { crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat };
    let site = crate::palw_court_v2::check_named_tir_root_for_claim_v1(state, &control.claim, control.leaf,
        &control.root, control.arity, rules.court.dissection_arity(), &rules.court, rules.step_ladder, prompt)
        .map_err(|e| e.to_string())?;
    Ok(CheckedSharedTirRootV1 { control, site })
}
