//! Immutable semantic identities carried by a legacy class's signed canonical registration.
//!
//! The operand inventory authenticates model operands, not tokenizer/configuration metadata.
//! Retaining these on-chain statements closes pruning ambiguity; it does not assert that a
//! tokenizer table was derived correctly or force model distribution (ADR-0177 D7).

use crate::{Hash64, palw_v2::PalwJobContextV2};

/// The meaning of a named prefix is immutable class configuration, independent of its table.
/// None in a snapshot is historical/unqualified and cannot be retroactively reinterpreted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwPrefixStateRelationV1 {
    DenseInt32KvV1 = 1,
    Qwen36HybridCacheV1 = 2,
    Base0Int8KvV1 = 3,
}

impl PalwPrefixStateRelationV1 {
    /// The relation is admitted only against the class's authenticated canonical graph.
    /// Cache-write kernel identity is part of that graph; an Int32 step lane alone cannot
    /// distinguish an i8 cache from a dense A16 cache.
    pub fn check_profile(self, profile: &crate::palw_step::PalwShapeProfileV3) -> Result<(), &'static str> {
        use crate::palw_step::{PalwStepNodeRoleV1 as Role, kernel_semantics_id_v1 as kid};
        use crate::palw_step_refute::*;
        profile.validate_shape().map_err(|_| "the state relation needs a valid canonical graph")?;
        let role = |r| {
            let mut nodes = profile.attn_nodes.iter().filter(|n| n.role == r);
            let node = nodes.next()?;
            if nodes.next().is_some() {
                return None;
            }
            Some(node.kernel_semantics_id)
        };
        match self {
            Self::DenseInt32KvV1 => {
                crate::palw_prefix_state_court_v1::PalwPrefixKvPlanV1::of_profile(profile, 1)?;
                if role(Role::KCacheWrite) != Some(kid(KDESC_A16_ROPE))
                    || role(Role::VCacheWrite) != Some(kid(KDESC_A16_MATMUL_REQUANT))
                {
                    return Err("the dense relation needs the canonical integer A16 cache-write kernels");
                }
            }
            Self::Base0Int8KvV1 => {
                crate::palw_prefix_state_court_v1::PalwPrefixKvPlanV1::of_profile(profile, 1)?;
                if role(Role::KCacheWrite) != Some(kid(KDESC_BASE0_REQUANTIZE))
                    || role(Role::VCacheWrite) != Some(kid(KDESC_BASE0_REQUANTIZE))
                {
                    return Err("the i8 relation needs the canonical BASE0 narrowed cache-write kernels");
                }
            }
            Self::Qwen36HybridCacheV1 => {
                crate::palw_prefix_family_state_v1::PalwQwen36PrefixCachePlanV1::new(profile, 1)
                    .map_err(|_| "the hybrid relation needs the exact Q36 cache geometry")?;
                if profile.gdn_layer_exists() {
                    let mut gdn = profile.gdn_nodes.iter().filter(|n| n.kernel_semantics_id == kid(KDESC_Q36_GDN_STEP));
                    let node = gdn.next().ok_or("the hybrid relation needs the canonical Q36 recurrence")?;
                    if gdn.next().is_some() || node.input_refs.len() != 5 {
                        return Err("the Q36 recurrence is not unique with five sources");
                    }
                    let mut conv = profile.gdn_nodes.iter().filter(|n| n.kernel_semantics_id == kid(KDESC_Q36_SSM_CONV));
                    let node = conv.next().ok_or("the hybrid relation needs the canonical Q36 convolution")?;
                    if conv.next().is_some() || node.input_refs.len() != 3 {
                        return Err("the Q36 convolution is not unique with three sources");
                    }
                }
                if (0..profile.layer_count).any(|l| profile.layer_kind(l) == crate::palw_step::PalwLayerKindV1::Attention)
                    && role(Role::KCacheWrite) != Some(kid(KDESC_Q36_ROPE_PARTIAL))
                {
                    return Err("the hybrid relation needs the Q36 attention cache-write kernel");
                }
            }
        }
        Ok(())
    }
}

pub const PALW_CLASS_SEMANTICS_VERSION_V1: u16 = 1;
pub const PALW_CARRIAGE_CLASS_SEMANTICS_TAIL_V1: u8 = 0xF7;

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
    /// Exact prospective raw-file length, authenticated only when its legacy hash completes.
    pub raw_tokenizer_bytes: Option<u64>,
    /// Maximum parser object-key plus vocabulary/added-id events. Understatement is an
    /// objective work-contract fault only after the exact original raw hash completes.
    pub tokenizer_key_events: Option<u64>,
    pub prefix_state_relation: Option<PalwPrefixStateRelationV1>,
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
            raw_tokenizer_bytes: None,
            tokenizer_key_events: None,
            prefix_state_relation: None,
        }
    }

    pub fn well_formed_for(&self, class_id: &Hash64, artifact_root: &Hash64) -> bool {
        self.version == PALW_CLASS_SEMANTICS_VERSION_V1
            && self.class_id == *class_id
            && self.artifact_root == *artifact_root
            && self.token_table_root != Some(Hash64::default())
            && (self.rendering_rule.is_none() || self.token_table_root.is_some())
            && (self.rendering_rule.is_some() == self.raw_tokenizer_bytes.is_some())
            && (self.rendering_rule.is_some() == self.tokenizer_key_events.is_some())
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
    pub raw_tokenizer_bytes: u64,
    pub tokenizer_key_events: u64,
    pub prefix_state_relation: Option<PalwPrefixStateRelationV1>,
}

pub const PALW_CLASS_SEMANTICS_SIGNATURE_CONTEXT_V1: &[u8] = b"misaka/palw/class-semantics/v1";

impl PalwClassSemanticsDeclarationV1 {
    pub fn as_bound_row(&self, bound_from_daa: u64) -> PalwClassSemanticsV1 {
        PalwClassSemanticsV1 {
            version: self.version,
            class_id: self.class_id,
            artifact_root: self.artifact_root,
            bound_from_daa,
            model_profile_id: self.model_profile_id,
            runtime_manifest_hash: self.runtime_manifest_hash,
            runtime_class_id: self.runtime_class_id,
            tokenizer_id: self.tokenizer_id,
            cu_ruleset_id: self.cu_ruleset_id,
            token_table_root: Some(self.token_table_root),
            rendering_rule: Some(self.rendering_rule),
            raw_tokenizer_bytes: Some(self.raw_tokenizer_bytes),
            tokenizer_key_events: Some(self.tokenizer_key_events),
            prefix_state_relation: self.prefix_state_relation,
        }
    }

    pub fn check(&self) -> Result<(), &'static str> {
        if self.version != PALW_CLASS_SEMANTICS_VERSION_V1
            || self.tokenizer_id == Hash64::default()
            || self.token_table_root == Hash64::default()
        {
            return Err("a semantic declaration needs version1, a nonzero tokenizer and a canonical table root");
        }
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
    fn prospective_state_relations_authenticate_the_cache_kernel_family() {
        let dense = crate::palw_qwen25_profile::qwen25_a16_profile_v2(crate::palw_qwen25_profile::QWEN25_1_5B_A16).unwrap();
        let floor = crate::palw_base0_profile::base0_profile_v1(crate::palw_base0_profile::PALW_RC_BASE0_GEOMETRY).unwrap();
        let hybrid = crate::palw_qwen36_profile::qwen36_profile_v2(crate::palw_qwen36_profile::QWEN36_35B_A3B).unwrap();
        for (relation, profile) in [
            (PalwPrefixStateRelationV1::DenseInt32KvV1, &dense),
            (PalwPrefixStateRelationV1::Base0Int8KvV1, &floor),
            (PalwPrefixStateRelationV1::Qwen36HybridCacheV1, &hybrid),
        ] {
            relation.check_profile(profile).unwrap();
            let mut forged = profile.clone();
            for n in &mut forged.attn_nodes {
                if n.role == crate::palw_step::PalwStepNodeRoleV1::KCacheWrite {
                    n.kernel_semantics_id = Hash64::from_u64_word(0xbad);
                }
            }
            assert!(relation.check_profile(&forged).is_err());
        }
        assert!(PalwPrefixStateRelationV1::DenseInt32KvV1.check_profile(&floor).is_err());
        assert!(PalwPrefixStateRelationV1::DenseInt32KvV1.check_profile(&hybrid).is_err());
        assert!(PalwPrefixStateRelationV1::Base0Int8KvV1.check_profile(&dense).is_err());
        assert!(PalwPrefixStateRelationV1::Qwen36HybridCacheV1.check_profile(&dense).is_err());
    }

    #[test]
    fn registration_semantics_bind_every_identity_and_never_nonce_or_prompt() {
        let mut ctx = crate::palw_attempt_rules_v1::floor_binding_for_tests_v1(
            &Hash64::from_u64_word(1),
            crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        )
        .job_context;
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
