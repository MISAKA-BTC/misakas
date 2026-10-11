//! Immutable semantic identities carried by a legacy class's signed canonical registration.
//!
//! The operand inventory authenticates model operands, not tokenizer/configuration metadata.
//! Retaining these on-chain statements closes pruning ambiguity; it does not assert that a
//! tokenizer table was derived correctly or force model distribution (ADR-0177 D7).

use crate::{Hash64, palw_v2::PalwJobContextV2};

pub const PALW_CLASS_SEMANTICS_VERSION_V1: u16 = 1;
pub const PALW_CARRIAGE_CLASS_SEMANTICS_TAIL_V1: u8 = 0xF3;

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwClassSemanticsV1 {
    pub version: u16,
    pub class_id: Hash64,
    pub artifact_root: Hash64,
    /// Earliest accepted claim eligible for this prospective registration statement.
    pub bound_from_daa: u64,
    pub model_profile_id: Hash64,
    pub runtime_manifest_hash: Hash64,
    pub runtime_class_id: Hash64,
    pub tokenizer_id: Hash64,
    pub cu_ruleset_id: Hash64,
    /// None until an independently signed immutable table descriptor is admitted. A job's
    /// self-selected table root is not a registered canonical table.
    pub token_table_root: Option<Hash64>,
    /// A prospective canonical rendering rule; None is an unqualified historical statement.
    pub rendering_rule: Option<crate::palw_tokenizer_rendering_v1::PalwTokenizerRenderingRuleV1>,
}

impl PalwClassSemanticsV1 {
    pub fn of_registration(class_id: Hash64, artifact_root: Hash64, canonical: &PalwJobContextV2, bound_from_daa: u64) -> Self {
        Self {
            version: PALW_CLASS_SEMANTICS_VERSION_V1,
            class_id,
            artifact_root,
            bound_from_daa,
            model_profile_id: canonical.model_profile_id,
            runtime_manifest_hash: canonical.runtime_manifest_hash,
            runtime_class_id: canonical.runtime_class_id,
            tokenizer_id: canonical.tokenizer_id,
            cu_ruleset_id: canonical.cu_ruleset_id,
            token_table_root: None,
            rendering_rule: None,
        }
    }

    pub fn well_formed_for(&self, class_id: &Hash64, artifact_root: &Hash64) -> bool {
        self.version == PALW_CLASS_SEMANTICS_VERSION_V1
            && self.class_id == *class_id && self.artifact_root == *artifact_root
            && self.token_table_root != Some(Hash64::default())
            && (self.rendering_rule.is_none() || self.token_table_root.is_some())
    }

    /// Each stored value is the registration's exact statement, including zero; a caller
    /// must explicitly handle an unlisted pre-fence class rather than substitute local facts.
    /// The public immutable snapshot's digest; the state's claim-keyed map binds it to a claim.
    pub fn commitment(&self) -> Hash64 {
        let mut h = blake2b_simd::Params::new().hash_length(64).key(b"PalwClassSemantics/v1").to_state();
        h.update(&borsh::to_vec(self).expect("a semantic snapshot has an infallible Vec writer"));
        Hash64::from_bytes(h.finalize().as_bytes().try_into().expect("the requested hash length"))
    }

    pub fn matches_context(&self, ctx: &PalwJobContextV2) -> bool {
        ctx.shape_profile_id == self.class_id
            && ctx.model_profile_id == self.model_profile_id
            && ctx.runtime_manifest_hash == self.runtime_manifest_hash
            && ctx.runtime_class_id == self.runtime_class_id
            && ctx.tokenizer_id == self.tokenizer_id
            && ctx.cu_ruleset_id == self.cu_ruleset_id
    }
}

/// A registrant's prospective assignment, not proof of a historical tokenizer/config binding.
/// No caller-supplied timestamp can make the statement apply before its accepting block.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwClassSemanticsDeclarationV1 {
    pub version: u16,
    pub class_id: Hash64,
    pub artifact_root: Hash64,
    pub model_profile_id: Hash64,
    pub runtime_manifest_hash: Hash64,
    pub runtime_class_id: Hash64,
    pub tokenizer_id: Hash64,
    pub cu_ruleset_id: Hash64,
    pub token_table_root: Hash64,
    pub rendering_rule: crate::palw_tokenizer_rendering_v1::PalwTokenizerRenderingRuleV1,
}

pub const PALW_CLASS_SEMANTICS_SIGNATURE_CONTEXT_V1: &[u8] = b"misaka/palw/class-semantics/v1";

impl PalwClassSemanticsDeclarationV1 {
    pub fn as_bound_row(&self, bound_from_daa: u64) -> PalwClassSemanticsV1 {
        PalwClassSemanticsV1 {
            version: self.version, class_id: self.class_id, artifact_root: self.artifact_root,
            bound_from_daa, model_profile_id: self.model_profile_id,
            runtime_manifest_hash: self.runtime_manifest_hash, runtime_class_id: self.runtime_class_id,
            tokenizer_id: self.tokenizer_id, cu_ruleset_id: self.cu_ruleset_id,
            token_table_root: Some(self.token_table_root),
            rendering_rule: Some(self.rendering_rule),
        }
    }

    pub fn check(&self) -> Result<(), &'static str> {
        if self.version != PALW_CLASS_SEMANTICS_VERSION_V1 || self.tokenizer_id == Hash64::default()
            || self.token_table_root == Hash64::default()
        { return Err("a semantic declaration needs version1, a nonzero tokenizer and a canonical table root"); }
        Ok(())
    }
}

pub fn palw_class_semantics_message_v1(network_domain: Hash64, declaration: &PalwClassSemanticsDeclarationV1) -> Hash64 {
    let mut h = blake2b_simd::Params::new().hash_length(64).personal(b"MISAKA_PALW_V1").to_state();
    h.update(b"class_semantics_declaration/v1");
    h.update(network_domain.as_byte_slice());
    h.update(&borsh::to_vec(declaration).expect("a semantic declaration has an infallible Vec writer"));
    Hash64::from_bytes(h.finalize().as_bytes().try_into().expect("the requested hash length"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registration_semantics_bind_every_identity_and_never_nonce_or_prompt() {
        let mut ctx = crate::palw_attempt_rules_v1::floor_binding_for_tests_v1(
            &Hash64::from_u64_word(1), crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat).job_context;
        ctx.tokenizer_id = Hash64::from_u64_word(70);
        let row = PalwClassSemanticsV1::of_registration(ctx.shape_profile_id, Hash64::from_u64_word(91), &ctx, 7);
        assert!(row.matches_context(&ctx));
        for field in 0..6 {
            let mut changed = ctx.clone();
            let other = Hash64::from_u64_word(0xBAD);
            match field {
                0 => changed.shape_profile_id = other,
                1 => changed.model_profile_id = other,
                2 => changed.runtime_manifest_hash = other,
                3 => changed.runtime_class_id = other,
                4 => changed.tokenizer_id = other,
                _ => changed.cu_ruleset_id = other,
            }
            assert!(!row.matches_context(&changed));
        }
        ctx.job_id = Hash64::from_u64_word(100);
        ctx.prompt_token_ids_hash = Hash64::from_u64_word(101);
        assert!(row.matches_context(&ctx));
        assert!(row.well_formed_for(&row.class_id, &row.artifact_root));
        let mut bad = row.clone();
        bad.token_table_root = Some(Hash64::default());
        assert!(!bad.well_formed_for(&row.class_id, &row.artifact_root));
    }
}
