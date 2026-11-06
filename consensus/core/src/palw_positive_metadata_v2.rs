//! Read-only native metadata for the Positive result lane.
//!
//! A peer's job envelope or Result is never an expected job. TIR registration
//! already roots the program, its attempt facts and the layout digest. Only the
//! bounded layout preimage is needed in addition to those existing rows. This
//! reader neither admits a claim nor grants a role, reservation, reward or credit.
//! The caller must borrow the actual current-prefix state from its native owner.

use crate::{
    Hash64,
    header::Header,
    palw_class_semantics_v1::PalwClassSemanticsV1,
    palw_prompt_ids_v1::PalwPromptIdsFormV1,
    palw_state_v2::{PalwBondKeyV2, PalwChainStateV2, PalwClassStateV2},
    palw_tir_admission_v1::{PALW_TIR_CLASS_RECORD_VERSION_V1, PalwTirClassRecordV1},
    palw_tir_class_v1::{
        PALW_TIR_CLASS_ID_DOMAIN_V1, PALW_TIR_CLASS_VERSION_V1, PALW_TIR_LAYOUT_DOMAIN_V1, PALW_TIR_LAYOUT_VERSION_V1, PalwTirLayoutV1,
    },
    palw_v2::PalwJobContextV2,
};
use misaka_palw_positive_contract::identity::{PUBKEY_BYTES, PrincipalId, principal};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PositiveMetadataLimitsV2 {
    pub max_program_bytes: u32,
    /// Combined commit/state layout entries, checked before serialization.
    pub max_layout_entries: u32,
    pub max_prompt_tokens: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PositiveMetadataErrorV2 {
    Budget,
    MissingClass,
    MissingTirClass,
    MissingBond,
    Key,
    Version,
    Program,
    Layout,
    ClassBinding,
    Semantics,
    UnsupportedAttempt,
    Prompt,
}

/// Borrowed registry metadata. Class existence is not an admission decision:
/// a stopped class can still be needed to adjudicate its historical claims.
pub struct PositiveTirMetadataV2<'s, 'l> {
    class: &'s PalwClassStateV2,
    registered: &'s PalwTirClassRecordV1,
    semantics: Option<&'s PalwClassSemanticsV1>,
    layout: &'l PalwTirLayoutV1,
    limits: PositiveMetadataLimitsV2,
}

impl<'s, 'l> PositiveTirMetadataV2<'s, 'l> {
    pub fn resolve(
        current_state: &'s PalwChainStateV2,
        class_id: Hash64,
        layout_preimage: &'l PalwTirLayoutV1,
        limits: PositiveMetadataLimitsV2,
    ) -> Result<Self, PositiveMetadataErrorV2> {
        let class = current_state.class(&class_id).ok_or(PositiveMetadataErrorV2::MissingClass)?;
        let registered = current_state.tir_class_v1(&class_id).ok_or(PositiveMetadataErrorV2::MissingTirClass)?;
        validate_parts(class_id, class, registered, layout_preimage, limits)?;
        let semantics = current_state.class_semantics_v1(&class_id);
        if semantics.is_some_and(|row| !row.well_formed_for(&class_id, &class.artifact_root)) {
            return Err(PositiveMetadataErrorV2::Semantics);
        }
        Ok(Self { class, registered, semantics, layout: layout_preimage, limits })
    }

    pub fn class(&self) -> &'s PalwClassStateV2 {
        self.class
    }
    pub fn registered(&self) -> &'s PalwTirClassRecordV1 {
        self.registered
    }
    pub fn layout(&self) -> &'l PalwTirLayoutV1 {
        self.layout
    }
    /// Optional prospective registered semantics, not defaults from a peer or
    /// local runtime. Attempt-yardstick identities are intentionally different.
    pub fn semantics(&self) -> Option<&'s PalwClassSemanticsV1> {
        self.semantics
    }

    /// Reuses the production TIR attempt derivation, with no execution trace.
    ///
    /// This is a deterministic calculation, NOT an authenticated origin token.
    /// The native owner must independently resolve this header, its accepted
    /// ancestry, producer bond, network and the two rules at its OWN DAA. Never
    /// pass a peer envelope/seed/prompt/canonical pair as those expectations.
    /// The origin must be independent of the Result carrier: pre-PoW hashing
    /// includes the transaction Merkle root, so using that same carrier's block
    /// would introduce a Result -> TxID -> header -> job -> Result cycle. The
    /// existing producer avoids that cycle with a post-PoW header commitment.
    /// For wide programs the caller must additionally reserve bounded program decode/copy and
    /// full-record derivation work within `limits`; these local caps are not an allocator lease.
    pub fn derive_attempt(
        &self,
        native_origin: &Header,
        native_network_domain: Hash64,
        native_producer_bond: &PalwBondKeyV2,
        native_prompt_form: PalwPromptIdsFormV1,
        native_prefill_draw: bool,
    ) -> Result<PositiveAttemptMetadataV2, PositiveMetadataErrorV2> {
        let mut derived = self.derive_job_for_anchor(
            || {
                crate::palw_attempt_v2::palw_job_anchor_v1(
                    native_network_domain,
                    crate::hashing::header::pre_pow_hash_64(native_origin),
                    self.registered.facts.class_id,
                    &native_producer_bond.0,
                    crate::palw_attempt_v2::palw_nonce_bucket_v1(native_origin.nonce),
                )
            },
            native_prompt_form,
        )?;
        derived.context = crate::palw_attempt_v2::palw_attempt_job_v1(derived.context, native_prefill_draw);
        Ok(derived)
    }

    /// RFC-0045 full canonical job, anchored to result-free Assignment terms.
    /// This deterministic derivation grants no accepted Assignment/role/funds
    /// authority; native intake must authenticate those independently.
    pub fn derive_assignment_job(
        &self,
        assignment: misaka_palw_positive_contract::assignment::AssignmentId,
        native_prompt_form: PalwPromptIdsFormV1,
    ) -> Result<PositiveAttemptMetadataV2, PositiveMetadataErrorV2> {
        self.derive_job_for_anchor(|| Hash64::from_bytes(assignment.0.0), native_prompt_form)
    }

    fn derive_job_for_anchor(
        &self,
        anchor: impl FnOnce() -> Hash64,
        native_prompt_form: PalwPromptIdsFormV1,
    ) -> Result<PositiveAttemptMetadataV2, PositiveMetadataErrorV2> {
        let facts = &self.registered.facts;
        let canonical = crate::palw_tir_attempt_v1::palw_tir_attempt_canonical_of_v1(facts.max_context)
            .ok_or(PositiveMetadataErrorV2::UnsupportedAttempt)?;
        if canonical.0 > self.limits.max_prompt_tokens || facts.max_context.checked_add(1).is_none() {
            return Err(PositiveMetadataErrorV2::Budget);
        }
        // Deny-only discriminator for the known wide identity in an already rooted canonical
        // V1 program: logits_scheme_id is its final [u8;64] field. This suffix is NOT profile
        // authentication or acceptance; wide derivation below still checks the entire record.
        // Keep legacy derivation exact for every old committable output, including I8/I16.
        let profile = if self.registered.program.ends_with(&misaka_palw_tir::program::LOGITS_I64_Q24_SCHEME_V2) {
            validate_parts(facts.class_id, self.class, self.registered, self.layout, self.limits)?;
            let limits = crate::palw_tir_class_record_v2::TirClassRecordProfileLimitsV2 {
                max_program_bytes: usize::try_from(self.limits.max_program_bytes).map_err(|_| PositiveMetadataErrorV2::Budget)?,
                max_layout_entries: usize::try_from(self.limits.max_layout_entries).map_err(|_| PositiveMetadataErrorV2::Budget)?,
            };
            Some(
                crate::palw_tir_class_record_v2::ProfiledTirClassRecordV2::resolve_record(
                    facts.class_id,
                    self.registered,
                    self.layout,
                    self.class.artifact_root,
                    limits,
                )
                .map_err(|e| match e {
                    crate::palw_tir_class_record_v2::TirClassRecordProfileErrorV2::Budget => PositiveMetadataErrorV2::Budget,
                    _ => PositiveMetadataErrorV2::ClassBinding,
                })?,
            )
        } else {
            None
        };
        let anchor = anchor();
        let (context, prompt) = match profile {
            Some(profile) => profile.job_for_anchor(&anchor, native_prompt_form),
            None => crate::palw_tir_attempt_v1::palw_tir_attempt_job_for_anchor_of_v1(facts, &anchor, canonical, native_prompt_form),
        }
        .ok_or(PositiveMetadataErrorV2::Prompt)?;
        Ok(PositiveAttemptMetadataV2 { context, prompt })
    }
}

/// Derived bytes only. Constructing this does not certify that its supplied
/// origin was accepted or that any inference was performed.
pub struct PositiveAttemptMetadataV2 {
    context: PalwJobContextV2,
    prompt: Vec<u32>,
}
impl PositiveAttemptMetadataV2 {
    pub fn into_parts(self) -> (PalwJobContextV2, Vec<u32>) {
        (self.context, self.prompt)
    }
    pub fn context(&self) -> &PalwJobContextV2 {
        &self.context
    }
    pub fn prompt(&self) -> &[u32] {
        &self.prompt
    }
}

/// Resolves the exact key already registered to this native bond. Identity
/// alone does not establish an active/matured Miner/Panel/provider role, live
/// collateral, consent, capacity or a successful signature verification.
pub fn positive_registered_principal_v2(
    current_state: &PalwChainStateV2,
    bond: &PalwBondKeyV2,
) -> Result<PrincipalId, PositiveMetadataErrorV2> {
    let row = current_state.bond(bond).ok_or(PositiveMetadataErrorV2::MissingBond)?;
    let key: &[u8; PUBKEY_BYTES] = row.pubkey.as_slice().try_into().map_err(|_| PositiveMetadataErrorV2::Key)?;
    // The native operator key may differ from the registered work key.
    // Operator identity is authenticated by registration and is used separately
    // for pool exclusion/deduplication. It cannot be rederived from this key.
    // This read creates no role/eligibility or signature capability.
    Ok(principal(key))
}

fn validate_parts(
    class_id: Hash64,
    class: &PalwClassStateV2,
    registered: &PalwTirClassRecordV1,
    layout: &PalwTirLayoutV1,
    limits: PositiveMetadataLimitsV2,
) -> Result<(), PositiveMetadataErrorV2> {
    if limits.max_program_bytes == 0
        || limits.max_layout_entries == 0
        || limits.max_prompt_tokens == 0
        || registered.program_bytes == 0
        || registered.program_bytes > limits.max_program_bytes
        || registered.program.len() as u64 > u64::from(limits.max_program_bytes)
        || (layout.commit_tiles.len() as u64)
            .checked_add(layout.state_tiles.len() as u64)
            .is_none_or(|entries| entries > u64::from(limits.max_layout_entries))
        || layout.commit_tiles.len() as u64 > u64::from(registered.program_bytes)
        || layout.state_tiles.len() as u64 > u64::from(registered.program_bytes)
    {
        return Err(PositiveMetadataErrorV2::Budget);
    }
    if registered.version != PALW_TIR_CLASS_RECORD_VERSION_V1 || layout.version != PALW_TIR_LAYOUT_VERSION_V1 {
        return Err(PositiveMetadataErrorV2::Version);
    }
    registered.check_program_v1().map_err(|_| PositiveMetadataErrorV2::Program)?;
    if registered.facts.class_id != class_id || layout.max_context != registered.facts.max_context {
        return Err(PositiveMetadataErrorV2::ClassBinding);
    }
    let layout_bytes = borsh::to_vec(layout).map_err(|_| PositiveMetadataErrorV2::Layout)?;
    let layout_digest = keyed64(PALW_TIR_LAYOUT_DOMAIN_V1, &[&layout_bytes]);
    if layout_digest != registered.layout_digest {
        return Err(PositiveMetadataErrorV2::Layout);
    }
    // Same existing class-id recipe, without cloning the registered program.
    let derived = keyed64(
        PALW_TIR_CLASS_ID_DOMAIN_V1,
        &[
            &PALW_TIR_CLASS_VERSION_V1.to_le_bytes(),
            registered.graph_ir_root.as_byte_slice(),
            layout_digest.as_byte_slice(),
            class.artifact_root.as_byte_slice(),
            registered.tokenizer_id.as_byte_slice(),
        ],
    );
    if derived != class_id {
        return Err(PositiveMetadataErrorV2::ClassBinding);
    }
    Ok(())
}

fn keyed64(domain: &[u8], parts: &[&[u8]]) -> Hash64 {
    let mut hash = blake2b_simd::Params::new().hash_length(64).key(domain).to_state();
    for part in parts {
        hash.update(part);
    }
    Hash64::from_bytes(hash.finalize().as_bytes().try_into().expect("64-byte digest"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_state_v2::{PalwClassStatusV2, PalwPwuRuleV2};

    fn fixture() -> (Hash64, PalwClassStateV2, PalwTirClassRecordV1, PalwTirLayoutV1, PositiveMetadataLimitsV2) {
        let execution = crate::palw_tir_court_v1::test_support::tiny_execution(None);
        let mut definition = execution.binding.class;
        definition.layout.max_context = 64;
        let artifact = execution.binding.artifact_root;
        let class_id = definition.class_id(&artifact);
        let (record, _) = crate::palw_tir_admission_v1::palw_tir_class_record_v1(&definition, &artifact).unwrap();
        let class = PalwClassStateV2 {
            artifact_root: artifact,
            slash_value_per_pwu: 1,
            pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: 1 },
            status: PalwClassStatusV2::Active,
            registered_daa: 0,
            registrant_bond: None,
            fused_attention: false,
        };
        let limits = PositiveMetadataLimitsV2 { max_program_bytes: 1 << 20, max_layout_entries: 1 << 16, max_prompt_tokens: 64 };
        (class_id, class, record, definition.layout, limits)
    }

    #[test]
    fn authenticated_layout_does_not_clone_the_registered_program() {
        let (id, class, record, layout, limits) = fixture();
        let before = std::sync::Arc::strong_count(&record.program);
        validate_parts(id, &class, &record, &layout, limits).unwrap();
        assert_eq!(std::sync::Arc::strong_count(&record.program), before);
        // A historical dispute can still resolve its immutable class after a
        // stop. Status eligibility belongs to the caller's original contract.
        let frozen = PalwClassStateV2 { status: PalwClassStatusV2::Frozen { since_daa: 4 }, ..class };
        validate_parts(id, &frozen, &record, &layout, limits).unwrap();
    }

    #[test]
    fn substitutions_cannot_change_layout_weights_tokenizer_or_program() {
        let (id, class, record, layout, limits) = fixture();
        let mut changed = layout.clone();
        changed.checkpoint_interval += 1;
        assert_eq!(validate_parts(id, &class, &record, &changed, limits), Err(PositiveMetadataErrorV2::Layout));
        let changed_class = PalwClassStateV2 { artifact_root: Hash64::from_bytes([9; 64]), ..class.clone() };
        assert_eq!(validate_parts(id, &changed_class, &record, &layout, limits), Err(PositiveMetadataErrorV2::ClassBinding));
        let mut changed = record.clone();
        changed.tokenizer_id = Hash64::from_bytes([9; 64]);
        assert_eq!(validate_parts(id, &class, &changed, &layout, limits), Err(PositiveMetadataErrorV2::ClassBinding));
        let mut changed = record.clone();
        std::sync::Arc::make_mut(&mut changed.program)[0] ^= 1;
        assert_eq!(validate_parts(id, &class, &changed, &layout, limits), Err(PositiveMetadataErrorV2::Program));
    }

    #[test]
    fn resource_caps_precede_program_hashing_and_layout_serialization() {
        let (id, class, mut record, layout, limits) = fixture();
        std::sync::Arc::make_mut(&mut record.program)[0] ^= 1;
        assert_eq!(
            validate_parts(id, &class, &record, &layout, PositiveMetadataLimitsV2 { max_program_bytes: 0, ..limits }),
            Err(PositiveMetadataErrorV2::Budget)
        );
        let mut changed = layout.clone();
        changed.commit_tiles.resize(limits.max_layout_entries as usize + 1, 1);
        assert_eq!(validate_parts(id, &class, &record, &changed, limits), Err(PositiveMetadataErrorV2::Budget));
    }

    #[test]
    fn missing_native_registry_rows_never_fall_back_to_peer_facts() {
        let (id, _, _, layout, limits) = fixture();
        let empty = PalwChainStateV2::genesis();
        assert!(matches!(PositiveTirMetadataV2::resolve(&empty, id, &layout, limits), Err(PositiveMetadataErrorV2::MissingClass)));
        let bond = PalwBondKeyV2(crate::tx::TransactionOutpoint::new(crate::TransactionId::from_bytes([3; 64]), 0));
        assert_eq!(positive_registered_principal_v2(&empty, &bond), Err(PositiveMetadataErrorV2::MissingBond));
    }

    #[test]
    fn production_attempt_job_is_rederived_from_header_and_rooted_facts() {
        let (_, class, record, layout, limits) = fixture();
        let metadata = PositiveTirMetadataV2 { class: &class, registered: &record, semantics: None, layout: &layout, limits };
        let mut header: Header = (&crate::config::genesis::GENESIS).into();
        let network = Hash64::from_bytes([2; 64]);
        let bond = PalwBondKeyV2(crate::tx::TransactionOutpoint::new(crate::TransactionId::from_bytes([3; 64]), 1));
        let expected_anchor = crate::palw_attempt_v2::palw_job_anchor_v1(
            network,
            crate::hashing::header::pre_pow_hash_64(&header),
            record.facts.class_id,
            &bond.0,
            crate::palw_attempt_v2::palw_nonce_bucket_v1(header.nonce),
        );
        let expected = crate::palw_tir_attempt_v1::palw_tir_attempt_job_for_anchor_of_v1(
            &record.facts,
            &expected_anchor,
            crate::palw_tir_attempt_v1::palw_tir_attempt_canonical_of_v1(record.facts.max_context).unwrap(),
            PalwPromptIdsFormV1::Flat,
        )
        .unwrap();
        let derived = metadata.derive_attempt(&header, network, &bond, PalwPromptIdsFormV1::Flat, false).unwrap();
        assert_eq!(derived.context(), &expected.0);
        assert_eq!(derived.prompt(), expected.1);
        assert_eq!(derived.context().tokenizer_id, Hash64::default(), "attempt yardstick does not copy registry metadata");
        let draw = metadata.derive_attempt(&header, network, &bond, PalwPromptIdsFormV1::Flat, true).unwrap();
        assert_eq!(draw.context(), &crate::palw_attempt_v2::palw_attempt_job_v1(expected.0, true));
        header.hash_merkle_root = Hash64::from_bytes([7; 64]);
        let changed = metadata.derive_attempt(&header, network, &bond, PalwPromptIdsFormV1::Flat, true).unwrap();
        assert_ne!(draw.context().job_id, changed.context().job_id, "a Result in its own carrier would change its job");
        let restricted = PositiveTirMetadataV2 { limits: PositiveMetadataLimitsV2 { max_prompt_tokens: 1, ..limits }, ..metadata };
        assert!(matches!(
            restricted.derive_attempt(&header, network, &bond, PalwPromptIdsFormV1::Flat, true),
            Err(PositiveMetadataErrorV2::Budget)
        ));
    }
    // DATA fixtures only: no registration/admission or native accepted capability is created.
    fn output_profile_fixture(
        dtype: misaka_palw_tir::DType,
    ) -> (Hash64, PalwClassStateV2, PalwTirClassRecordV1, PalwTirLayoutV1, PositiveMetadataLimitsV2) {
        use misaka_palw_tir::{DType, Node, Prim, Ref};
        let (_, class, record, layout, mut limits) = fixture();
        let mut definition = crate::palw_tir_class_v1::PalwTirClassV1 {
            version: 1,
            program: record.program.as_ref().clone(),
            layout,
            tokenizer_id: record.tokenizer_id,
        };
        let mut program = definition.decode_program().unwrap();
        let post = &mut program.blocks[program.schedule.post as usize];
        let mut output = post.nodes[program.logits as usize].out.clone();
        output.dtype = dtype;
        post.nodes.push(Node { prim: Prim::Cast, inputs: vec![Ref::Node(program.logits)], out: output, commit: true });
        program.logits = (post.nodes.len() - 1) as u16;
        definition.layout.commit_tiles.push(4);
        if dtype == DType::I64 {
            program.logits_scheme_id = misaka_palw_tir::program::LOGITS_I64_Q24_SCHEME_V2;
            definition.layout.max_context = 2048;
            limits.max_prompt_tokens = 4096;
        }
        definition.program = program.encode();
        let record = if dtype == DType::I64 {
            crate::palw_tir_class_record_v2::ProfiledTirClassRecordV2::build(
                &definition,
                class.artifact_root,
                crate::palw_tir_class_record_v2::TirClassRecordProfileLimitsV2 {
                    max_program_bytes: limits.max_program_bytes as usize,
                    max_layout_entries: limits.max_layout_entries as usize,
                },
            )
            .unwrap()
            .into_storage_row()
        } else {
            crate::palw_tir_admission_v1::palw_tir_class_record_v1(&definition, &class.artifact_root).unwrap().0
        };
        (definition.class_id(&class.artifact_root), class, record, definition.layout, limits)
    }

    #[test]
    fn explicit_wide_attempt_preserves_full_original_job_and_only_changes_the_trace_profile() {
        let (_, class, record, layout, limits) = output_profile_fixture(misaka_palw_tir::DType::I64);
        assert!(!record.facts.tiled, "the old bool is not a tiled-family classifier");
        let metadata = PositiveTirMetadataV2 { class: &class, registered: &record, semantics: None, layout: &layout, limits };
        let header: Header = (&crate::config::genesis::GENESIS).into();
        let network = Hash64::from_bytes([2; 64]);
        let bond = PalwBondKeyV2(crate::tx::TransactionOutpoint::new(crate::TransactionId::from_bytes([3; 64]), 1));
        let anchor = crate::palw_attempt_v2::palw_job_anchor_v1(
            network,
            crate::hashing::header::pre_pow_hash_64(&header),
            record.facts.class_id,
            &bond.0,
            crate::palw_attempt_v2::palw_nonce_bucket_v1(header.nonce),
        );
        for form in [PalwPromptIdsFormV1::Flat, PalwPromptIdsFormV1::MerkleV1] {
            let (mut expected, ids) =
                crate::palw_tir_attempt_v1::palw_tir_attempt_job_for_anchor_of_v1(&record.facts, &anchor, (255, 2), form).unwrap();
            assert_ne!(expected.trace_scheme_id.as_bytes(), misaka_palw_tir::program::LOGITS_I64_Q24_SCHEME_V2);
            expected.trace_scheme_id = Hash64::from_bytes(misaka_palw_tir::program::LOGITS_I64_Q24_SCHEME_V2);
            for draw in [false, true] {
                let derived = metadata.derive_attempt(&header, network, &bond, form, draw).unwrap();
                assert_eq!(derived.context(), &crate::palw_attempt_v2::palw_attempt_job_v1(expected.clone(), draw));
                assert_eq!(derived.prompt(), ids);
                assert_eq!(derived.context().max_context_tokens, 2049);
                assert_eq!(derived.context().shape_profile_id, record.facts.class_id);
                assert_eq!(derived.context().tokenizer_id, Hash64::default());
                assert_eq!(derived.context().cu_ruleset_id, Hash64::default());
            }
        }
    }

    #[test]
    fn old_committable_i8_i16_attempts_retain_exact_legacy_contexts() {
        for dtype in [misaka_palw_tir::DType::I8, misaka_palw_tir::DType::I16] {
            let (_, class, record, layout, limits) = output_profile_fixture(dtype);
            let metadata = PositiveTirMetadataV2 { class: &class, registered: &record, semantics: None, layout: &layout, limits };
            let header: Header = (&crate::config::genesis::GENESIS).into();
            let network = Hash64::from_bytes([2; 64]);
            let bond = PalwBondKeyV2(crate::tx::TransactionOutpoint::new(crate::TransactionId::from_bytes([3; 64]), 1));
            let anchor = crate::palw_attempt_v2::palw_job_anchor_v1(
                network,
                crate::hashing::header::pre_pow_hash_64(&header),
                record.facts.class_id,
                &bond.0,
                crate::palw_attempt_v2::palw_nonce_bucket_v1(header.nonce),
            );
            for form in [PalwPromptIdsFormV1::Flat, PalwPromptIdsFormV1::MerkleV1] {
                let expected =
                    crate::palw_tir_attempt_v1::palw_tir_attempt_job_for_anchor_of_v1(&record.facts, &anchor, (7, 2), form).unwrap();
                let actual = metadata.derive_attempt(&header, network, &bond, form, true).unwrap();
                assert_eq!(actual.context(), &crate::palw_attempt_v2::palw_attempt_job_v1(expected.0, true));
                assert_eq!(actual.prompt(), expected.1);
            }
        }
    }

    #[test]
    fn wide_job_derivation_rechecks_all_row_fields_and_rejects_substitutions_and_caps() {
        let (id, class, record, layout, limits) = output_profile_fixture(misaka_palw_tir::DType::I64);
        let header: Header = (&crate::config::genesis::GENESIS).into();
        let network = Hash64::from_bytes([2; 64]);
        let bond = PalwBondKeyV2(crate::tx::TransactionOutpoint::new(crate::TransactionId::from_bytes([3; 64]), 1));
        let derive = |class: &PalwClassStateV2, row: &PalwTirClassRecordV1, layout: &PalwTirLayoutV1, limits| {
            PositiveTirMetadataV2 { class, registered: row, semantics: None, layout, limits }.derive_attempt(
                &header,
                network,
                &bond,
                PalwPromptIdsFormV1::Flat,
                false,
            )
        };
        // Each forged cached field still passes the former hash/layout-only metadata check.
        // Full V2 row derivation must independently refuse it before generating expected jobs.
        let mut substitutions = vec![record.clone(); 5];
        substitutions[0].facts.token_bound += 1;
        substitutions[1].facts.tiled = true;
        substitutions[2].facts.held = true;
        substitutions[3].logits_vocab += 1;
        substitutions[4].dissected.clear();
        // Ensure a nontrivial row change even if this fixture has no history-reducing cone.
        substitutions[4].dissected.push((255, 65535));
        for changed in substitutions {
            validate_parts(id, &class, &changed, &layout, limits).unwrap();
            assert!(matches!(derive(&class, &changed, &layout, limits), Err(PositiveMetadataErrorV2::ClassBinding)));
        }
        let foreign = PalwClassStateV2 { artifact_root: Hash64::from_bytes([9; 64]), ..class.clone() };
        assert!(matches!(derive(&foreign, &record, &layout, limits), Err(PositiveMetadataErrorV2::ClassBinding)));
        let mut changed_layout = layout.clone();
        changed_layout.checkpoint_interval += 1;
        assert!(matches!(derive(&class, &record, &changed_layout, limits), Err(PositiveMetadataErrorV2::Layout)));
        let mut changed = record.clone();
        std::sync::Arc::make_mut(&mut changed.program)[0] ^= 1;
        assert!(matches!(derive(&class, &changed, &layout, limits), Err(PositiveMetadataErrorV2::Program)));
        // Bounds win before hashing the deliberately corrupted program.
        let restricted = PositiveMetadataLimitsV2 { max_program_bytes: record.program_bytes - 1, ..limits };
        assert!(matches!(derive(&class, &changed, &layout, restricted), Err(PositiveMetadataErrorV2::Budget)));
        let restricted = PositiveMetadataLimitsV2 { max_prompt_tokens: 254, ..limits };
        assert!(matches!(derive(&class, &record, &layout, restricted), Err(PositiveMetadataErrorV2::Budget)));
    }

    #[test]
    fn assignment_anchor_keeps_full_canonical_counts_and_original_profiles() {
        use misaka_palw_positive_contract::{assignment::AssignmentId, identity::Digest};
        for dtype in [misaka_palw_tir::DType::I8, misaka_palw_tir::DType::I16, misaka_palw_tir::DType::I64] {
            let (_, class, record, layout, limits) = output_profile_fixture(dtype);
            let metadata = PositiveTirMetadataV2 { class: &class, registered: &record, semantics: None, layout: &layout, limits };
            let assignment = AssignmentId(Digest([0x45; 64]));
            let anchor = Hash64::from_bytes(assignment.0.0);
            for form in [PalwPromptIdsFormV1::Flat, PalwPromptIdsFormV1::MerkleV1] {
                let full = crate::palw_tir_attempt_v1::palw_tir_attempt_canonical_of_v1(record.facts.max_context).unwrap();
                let (mut expected, ids) =
                    crate::palw_tir_attempt_v1::palw_tir_attempt_job_for_anchor_of_v1(&record.facts, &anchor, full, form).unwrap();
                if dtype == misaka_palw_tir::DType::I64 {
                    expected.trace_scheme_id = Hash64::from_bytes(misaka_palw_tir::program::LOGITS_I64_Q24_SCHEME_V2);
                }
                let actual = metadata.derive_assignment_job(assignment, form).unwrap();
                assert_eq!(actual.context(), &expected);
                assert_eq!(actual.prompt(), ids);
                assert_eq!(actual.context().exact_decode_tokens, 2, "no primary prefill-draw shortening");
                assert_eq!(actual.context().max_context_tokens, record.facts.max_context + 1);
                assert_eq!(actual.context().tokenizer_id, Hash64::default());
                assert_eq!(actual.context().cu_ruleset_id, Hash64::default());
                let another = metadata.derive_assignment_job(AssignmentId(Digest([0x46; 64])), form).unwrap();
                assert_ne!(another.context().context_hash(), actual.context().context_hash());
            }
        }
    }
}
