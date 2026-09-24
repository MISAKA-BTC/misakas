//! Fixtures shared by the seat tests (SEAT-S1, S-2, S-4): the three families the live chain uses,
//! built the way kaspad's SDK builds their backends — `seat_material_binds_the_claim`'s, restated
//! here because an integration test cannot import another's helpers.

#![allow(dead_code)]

use std::sync::Arc;

use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1;
use kaspa_consensus_core::palw_qwen36_profile::PalwQwen36GeometryV1;
use kaspa_consensus_core::palw_step::{PALW_STEP_MAX_LEAVES, PalwShapeProfileV3};
use kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES;
use misaka_palw_base0::artifact::{Base0ArtifactV1, Base0ShapeV1, LN_THETA_10000_GEN_Q};
use misaka_palw_base0::backend::Base0Backend;
use misaka_palw_base0::qwen25_a16_backend::Qwen25A16Backend;
use misaka_palw_base0::qwen36_backend::Qwen36Backend;

pub const NETWORK: &[u8] = b"misaka-palw-rc";

/// The floor, resolved from nothing as kaspad's dense lineage resolves it, at the network's ladder
/// and prompt form.
pub fn floor_backend(form: PalwPromptIdsFormV1) -> Base0Backend {
    use misaka_palw_base0::classes::{canonical_class_by_model_id_v1, resolve_class_v1};
    let court = PalwCourtParamsV2::new(PALW_STEP_MAX_LEAVES, 4, 2).expect("the shipped court");
    let entry = canonical_class_by_model_id_v1(&court, "PALW-BASE-0/rc").expect("the floor is registered");
    let root = misaka_palw_base0::rc::palw_rc_base0_artifact_root_v1().expect("the floor's pinned root");
    Base0Backend::new(resolve_class_v1(&court, entry.class_id(), root, &[]).expect("the floor resolves from nothing"))
        .with_step_ladder_cap(court.max_step_leaf_count())
        .with_prompt_ids_form(form)
}

/// The held A16 fixture's geometry (graph-v7, the held map), at `vocab`.
pub fn a16_geometry(vocab: u32) -> PalwQwen25GeometryV1 {
    PalwQwen25GeometryV1 {
        layer_count: 2,
        hidden_dim: 32,
        ffn_dim: 64,
        attn_heads: 4,
        attn_kv_heads: 2,
        attn_head_dim: 8,
        vocab_size: vocab,
        n_ctx: 128,
        n_threads: 1,
        rms_eps_q: 1,
        tile_len: 4,
    }
}

pub fn a16_artifact(vocab: u32) -> Arc<Base0ArtifactV1> {
    let g = a16_geometry(vocab);
    let shape = Base0ShapeV1 {
        n_layers: g.layer_count as usize,
        n_heads: g.attn_heads as usize,
        n_kv_heads: g.attn_kv_heads as usize,
        d_head: g.attn_head_dim as usize,
        d_ff: g.ffn_dim as usize,
        vocab: g.vocab_size as usize,
        max_position: g.n_ctx as usize,
        ln_theta_gen_q: LN_THETA_10000_GEN_Q,
        eps_q: g.rms_eps_q,
    };
    Arc::new(
        Base0ArtifactV1::derive_deterministic(shape, 0x5A16)
            .expect("a valid shape")
            .with_a16_params(misaka_palw_base0::engine_a16::derived_a16_store(&shape))
            .expect("the derived store is sorted and unique"),
    )
}

pub fn a16_backend(artifact: &Arc<Base0ArtifactV1>, profile: &PalwShapeProfileV3, canonical: (u32, u32)) -> Qwen25A16Backend {
    Qwen25A16Backend::new(artifact.clone(), NETWORK.to_vec(), profile.clone(), canonical)
        .expect("the fixture's declaration is this engine's program")
        .with_step_ladder_cap(PALW_STEP_LEG_MAX_LEAVES)
        .with_prompt_ids_form(PalwPromptIdsFormV1::MerkleV1)
}

/// The Qwen3.6 dev fixture and its geometry (`fuzz_qwen36`'s tiny class).
pub fn qwen36_fixture() -> (Arc<misaka_palw_base0::qwen36::Qwen36ArtifactV1>, PalwQwen36GeometryV1) {
    let geometry = PalwQwen36GeometryV1 {
        layer_count: 4,
        full_attention_interval: 4,
        hidden_dim: 32,
        attn_heads: 4,
        attn_kv_heads: 2,
        attn_head_dim: 16,
        rope_dims: 4,
        rope_freq_base_bits: 0x4B18_9680,
        gdn_k_heads: 2,
        gdn_v_heads: 4,
        gdn_head_dim: 8,
        gdn_conv_kernel: 4,
        n_experts: 8,
        experts_per_token: 4,
        moe_dim: 16,
        shared_dim: 16,
        attn_output_gate: 1,
        vocab_size: 64,
        n_ctx: 8,
        n_threads: 1,
        rms_eps_q: 1,
        tile_len: 512,
    };
    (Arc::new(misaka_palw_base0::qwen36::qwen36_dev_fixture(4, 8)), geometry)
}

pub fn qwen36_backend(
    artifact: &Arc<misaka_palw_base0::qwen36::Qwen36ArtifactV1>,
    profile: &PalwShapeProfileV3,
    canonical: (u32, u32),
) -> Qwen36Backend {
    Qwen36Backend::from_registered_profile(artifact.clone(), NETWORK.to_vec(), profile.clone(), canonical)
        .expect("servable")
        .with_step_ladder_cap(PALW_STEP_LEG_MAX_LEAVES)
        .with_prompt_ids_form(PalwPromptIdsFormV1::MerkleV1)
}

/// A greedy free-prompt job of `class`, its prompt committed under `form` (the class's own).
pub fn fp_job(
    class: &PalwShapeProfileV3,
    form: PalwPromptIdsFormV1,
    prompt: &[usize],
    decode: u32,
) -> kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3 {
    use kaspa_consensus_core::palw_freeprompt_v3::{
        PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_PROMPT_MODE_USER, PALW_FP_V3_VERSION, PalwFreePromptJobV3,
    };
    use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
    use kaspa_hashes::Hash64;
    let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
    PalwFreePromptJobV3 {
        version: PALW_FP_V3_VERSION,
        network_domain: Hash64::from_u64_word(0xD0),
        class_id: class.shape_profile_id(),
        executor_bond: TransactionOutpoint::new(TransactionId::from_u64_word(0xB0), 0),
        executor_pubkey: vec![0x11; 32],
        operator_id: Hash64::from_u64_word(0x0B),
        anchor_block: Hash64::from_u64_word(0xA0),
        anchor_daa: 4242,
        job_nonce: [0x5A; 32],
        tokenizer_id: Hash64::default(),
        prompt_token_ids_hash: kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_commitment_v1(form, &ids)
            .expect("the ids commit"),
        prompt_tokens: prompt.len() as u32,
        decode_token_limit: decode,
        max_context_tokens: class.n_ctx,
        privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
        prompt_mode: PALW_FP_PROMPT_MODE_USER,
        sampling_seed: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
        temperature_q: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
    }
}
