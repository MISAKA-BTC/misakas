//! **ADR-0152 v3.1 J-5 (the audit's SPEC §4.3, addendum §4-bis.1): the attempt job, as the CHAIN
//! derives it.**
//!
//! An attempt claim's job is not the producer's to choose: the block's execution anchor names it,
//! and a seat refuses material whose job context is not the one the anchor implies
//! (`misaka-palw-base0`'s `verify_material`, "the whole job, not only its id"). Until F1 that rule
//! lived only in the producer's crate, so the chain could not ask it: a relabelled run — prompt A
//! executed, `job_id := anchor_B` written into the context — was a binding the chain had no way to
//! call wrong. This module is that rule in core, with every byte the producer derives moved here
//! unchanged (golden-tested against `base0_rc_job_v1` in `misaka-palw-base0`):
//!
//! * the counter-mode prompt loop ([`palw_attempt_prompt_ids_v1`], domain
//!   [`PALW_ATTEMPT_PROMPT_DOMAIN_V1`] — the floor's `PALW_BASE0_DOMAIN_JOB_PROMPT`, byte for byte);
//! * the canonical job a class is attempted at ([`palw_attempt_canonical_v1`]);
//! * the whole context an anchor implies ([`palw_attempt_context_v1`]) and its prompt root
//!   ([`palw_attempt_prompt_root_v1`]), the two halves of the identity check's J5.
//!
//! **F1-M (addendum §4-bis.1): one rule, `CoreV1`, for the floor and every model class.** A model
//! class's canonical job is the profile formula `(n_ctx/8 − 1, 2)` (admission pins it past
//! `palw_offence_attribution`), and its attempt context is the floor's construction over its own
//! profile — the model, runtime, tokenizer, nullifier, assignment and cu fields zero, the ceiling
//! `n_ctx`, the network `"misaka-palw-rc"` (the anchor already binds network and genesis) — so
//! nothing held only by an artifact enters what the chain checks, and a relabelled model-class run
//! fails J5 exactly as a floor one does. Producers and seats switch their `job_for_anchor` and every
//! `output_root` to this rule under the same fence ([`PalwAttemptRulesV1::CoreV1`]); on every other
//! network they keep their family's own (`Legacy`), and nothing here is consulted.
//!
//! The other pieces the rule reads, each moved from the producer's crate byte for byte:
//! [`palw_int_activation_leg_root_v1`] (J6), [`palw_canonical_checkpoint_profile_v1`] (J7) and
//! [`palw_attempt_rendered_output_v1`] (the one rendered rule `OutputMismatch` holds a claim to).

use crate::palw_step::PalwShapeProfileV3;
use crate::palw_v2::PalwJobContextV2;
use kaspa_hashes::Hash64;

/// **The attempt prompt's domain** — `misaka-palw-base0`'s `PALW_BASE0_DOMAIN_JOB_PROMPT`, moved
/// here with the byte string unchanged, so every floor prompt ever derived is still the one this
/// derives.
pub const PALW_ATTEMPT_PROMPT_DOMAIN_V1: &[u8] = b"misaka-palw/base0/rc-job-prompt/v1";

/// **The longest canonical prompt the identity check recomputes inline** (addendum §4-bis.2, J5b):
/// 4,096 ids is 513 BLAKE2b blocks — the floor (8), a 2k row (255) and the 8k row (1,023) all sit
/// under it. A longer prompt (the 2M row) is checked by `PromptNotAnchored` (F1-M).
pub const PALW_J5_INLINE_PROMPT_IDS_V1: u32 = 4096;

/// **The most prompt ids one block may spend recomputing a whole prompt root** (addendum §4-bis.3,
/// `PromptNotAnchored`'s `Whole` proof): one 2M row's canonical prefill (262,143 ids) per block.
pub const PALW_HEAVY_PROMPT_IDS_PER_BLOCK_V1: u64 = 1 << 18;

/// **Which attempt rule a producer or seat runs** — the family's own (`Legacy`: its prompt domain,
/// its context fields, its rendered-output hash) or the chain's (`CoreV1`, this module). Not
/// consensus state and never serialized: every node derives it from its params
/// ([`palw_attempt_rules_of_params_v1`]), so producers, seats and the chain's identity checks agree.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PalwAttemptRulesV1 {
    #[default]
    Legacy,
    CoreV1,
}

/// **`CoreV1` exactly where `Params::palw_offence_attribution` is armed** (testnet-12, from genesis);
/// `Legacy` on every other network, whose roots therefore do not move.
pub fn palw_attempt_rules_of_params_v1(params: &crate::config::params::Params) -> PalwAttemptRulesV1 {
    if params.palw_offence_attribution_fence().is_some() { PalwAttemptRulesV1::CoreV1 } else { PalwAttemptRulesV1::Legacy }
}

/// Domain of [`palw_int_activation_leg_root_v1`] — `misaka-palw-base0`'s
/// `PALW_BASE0_DOMAIN_ACTIVATION_LEG`, moved with the byte string unchanged.
pub const PALW_INT_ACTIVATION_LEG_DOMAIN_V1: &[u8] = b"misaka-palw/base0/activation-leg/v1";

/// **The integer classes' activation leg: the statement that the class taps nothing** — what every
/// producer of this tree files (the floor, A16 and Qwen3.6 all call it), moved from
/// `base0_activation_leg_root_v1` byte for byte. J6 holds a binding to it.
pub fn palw_int_activation_leg_root_v1(ctx: &PalwJobContextV2) -> Hash64 {
    let mut h = blake2b_simd::Params::new().hash_length(64).key(PALW_INT_ACTIVATION_LEG_DOMAIN_V1).to_state();
    h.update(ctx.context_hash().as_byte_slice());
    h.update(&(ctx.declared_prefill_tokens as u64).to_le_bytes());
    h.update(&(ctx.exact_decode_tokens as u64).to_le_bytes());
    h.update(b"no-taps");
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// **The checkpoint profile a class's producers file** — the integer-kv profile at interval 1, or,
/// for a class with recurrence heads (the hybrid, whose map is genesis-anchored), at `n_ctx`: the
/// rule the floor, A16 and Qwen3.6 producers apply today (`base0/produce.rs`,
/// `qwen36_checkpoint_profile_v1`), one predicate for all. J7 holds a binding to it.
pub fn palw_canonical_checkpoint_profile_v1(profile: &PalwShapeProfileV3) -> crate::palw_legs::PalwCheckpointProfileV1 {
    crate::palw_state_chunk_map::integer_kv_checkpoint_profile_v1(if profile.gdn_heads > 0 {
        profile.n_ctx.max(1)
    } else {
        crate::palw_state_chunk_map::PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1
    })
}

/// **The one rendered-output rule under `CoreV1`**: the empty rendering, the floor's
/// (`output_commitment_v2(ctx, ids, rendered_output_hash_v2(&[]))`). A family's keyed rendering is a
/// function of its ids too, so it adds nothing a consumer could not recompute — and one rule is what
/// lets `OutputMismatch` hold every class's claims to the same arithmetic.
pub fn palw_attempt_rendered_output_v1(_ids: &[u32]) -> Hash64 {
    crate::palw_v2::rendered_output_hash_v2(&[])
}

/// `output_root` of `ids` under `ctx` by the `CoreV1` rule.
pub fn palw_attempt_output_root_v1(ctx: &PalwJobContextV2, ids: &[u32]) -> Hash64 {
    crate::palw_v2::output_commitment_v2(&ctx.context_hash(), ids, &palw_attempt_rendered_output_v1(ids))
}

/// Ids per counter block: one 64-byte BLAKE2b output read as eight little-endian `u64` words.
const PALW_ATTEMPT_PROMPT_WORDS_PER_BLOCK_V1: u64 = 8;

fn prompt_block(anchor: &Hash64, counter: u64) -> [u8; 64] {
    let mut h = blake2b_simd::Params::new().hash_length(64).key(PALW_ATTEMPT_PROMPT_DOMAIN_V1).to_state();
    h.update(anchor.as_byte_slice());
    h.update(&counter.to_le_bytes());
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    out
}

fn prompt_word(block: &[u8; 64], word: usize, vocab: u64) -> u32 {
    let v = u64::from_le_bytes(block[word * 8..word * 8 + 8].try_into().expect("eight bytes"));
    // `vocab.max(1)`: the floor's own guard against a zero vocabulary, kept byte for byte.
    (v % vocab.max(1)) as u32
}

/// **The prompt an anchor names: `count` ids in counter mode** — block `c` is
/// `BLAKE2b-512_key(PALW_ATTEMPT_PROMPT_DOMAIN_V1)(anchor ‖ c_le)`, read as eight words, each
/// reduced modulo `vocab`. The floor's `base0_rc_job_v1` loop, moved here unchanged.
pub fn palw_attempt_prompt_ids_v1(anchor: &Hash64, vocab: u64, count: u32) -> Vec<u32> {
    palw_attempt_prompt_ids_range_v1(anchor, vocab, 0, u64::from(count))
}

/// **Ids `start..start + len` of the anchor's prompt**, computed from their own counter blocks
/// alone (`pos / 8`, word `pos % 8`) — `⌈len / 8⌉ + 1` hashes at most, whatever `start` is. Equal,
/// id for id, to the same range of [`palw_attempt_prompt_ids_v1`].
pub fn palw_attempt_prompt_ids_range_v1(anchor: &Hash64, vocab: u64, start: u64, len: u64) -> Vec<u32> {
    let mut out = Vec::with_capacity(usize::try_from(len).unwrap_or(0));
    let mut cached: Option<(u64, [u8; 64])> = None;
    for pos in start..start.saturating_add(len) {
        let counter = pos / PALW_ATTEMPT_PROMPT_WORDS_PER_BLOCK_V1;
        let block = match cached {
            Some((c, block)) if c == counter => block,
            _ => {
                let block = prompt_block(anchor, counter);
                cached = Some((counter, block));
                block
            }
        };
        out.push(prompt_word(&block, (pos % PALW_ATTEMPT_PROMPT_WORDS_PER_BLOCK_V1) as usize, vocab));
    }
    out
}

/// **Why a class is not attributable past `palw_offence_attribution`** (ADR-0152 v3.1 addendum
/// §4-bis.8) — the registration refusals, by name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwAttributableClassErrorV1 {
    /// Too narrow for the formula (`n_ctx < 16`): no canonical attempt job exists, so J5 could
    /// never be asked of its claims — `IdentityNotDerivable` must not be a registrant's way out of
    /// the identity rule.
    #[error("the class's context is too narrow for the canonical job formula (n_ctx/8 - 1, 2); J5 could never be asked of its claims")]
    TooNarrow,
    /// The registration names a canonical job that is not the formula's.
    #[error("the registered canonical job ({got_prefill}, {got_decode}) is not the formula's ({want_prefill}, {want_decode})")]
    CanonicalNotTheFormula { got_prefill: u32, got_decode: u32, want_prefill: u32, want_decode: u32 },
    /// The graph does not make its logits row a provable step output (`palw_logits_head_v1`) —
    /// every `Float32` class, and any whose last node is not a shipped head over the vocabulary.
    #[error("the class's logits row is not a provable step output (palw_logits_head_v1): LogitsNotStepOutput could not judge it")]
    HeadUnproven,
    /// The graph reaches a Kimi K3 kernel, for which no engine and no golden identity exist.
    #[error("the class reaches a Kimi K3 kernel, which no engine runs and no golden pins")]
    KimiKernel,
    /// A canonical prompt past J5b's inline bound committed in a form `PromptNotAnchored` cannot open.
    #[error("a canonical prompt of {prefill} ids is past J5b's inline bound and must be committed in the Merkle form")]
    WidePromptNotMerkle { prefill: u32 },
}

/// **What a class must be for every claim of it to be attributable** (ADR-0152 v3.1 addendum
/// §4-bis.8), checked at registration past `palw_offence_attribution`, beside
/// `verify_class_admission_v9`:
///
/// * (a) its canonical job is the formula's ([`palw_attempt_canonical_v1`]) — and a class too narrow
///   for the formula is refused, so no registrant escapes J5 by choosing a width
///   (`IdentityNotDerivable` stays a refusal for claims of classes registered before the fence);
/// * (b) its logits row is a provable step output ([`crate::palw_step::palw_logits_head_v1`]), which
///   also refuses `Float32`;
/// * (c) it reaches no Kimi K3 kernel (no engine, no golden identity);
/// * (d) a canonical prompt past [`PALW_J5_INLINE_PROMPT_IDS_V1`] is committed in the Merkle form
///   under the network's (`palw_prompt_ids_form_of_class_v1`), so `PromptNotAnchored` can open it.
///
/// `is_base` reads the floor's canonical job rather than the formula; the base class is a genesis
/// row and never reaches the registration gate, and testnet-12's genesis rows pass all four.
pub fn palw_attributable_class_v1(
    profile: &PalwShapeProfileV3,
    canonical: &PalwJobContextV2,
    is_base: bool,
    network_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) -> Result<(), PalwAttributableClassErrorV1> {
    use crate::palw_step_refute::{KDESC_KIMI_KDA_STEP, KDESC_KIMI_MLA_FUSED, KDESC_KIMI_MOE_COMBINE, KDESC_KIMI_ROUTER_TOPK};
    let (prefill, decode) = palw_attempt_canonical_v1(profile, is_base).ok_or(PalwAttributableClassErrorV1::TooNarrow)?;
    if (canonical.declared_prefill_tokens, canonical.exact_decode_tokens) != (prefill, decode) {
        return Err(PalwAttributableClassErrorV1::CanonicalNotTheFormula {
            got_prefill: canonical.declared_prefill_tokens,
            got_decode: canonical.exact_decode_tokens,
            want_prefill: prefill,
            want_decode: decode,
        });
    }
    crate::palw_step::palw_logits_head_v1(profile).ok_or(PalwAttributableClassErrorV1::HeadUnproven)?;
    let reachable = profile.reachable_kernel_ids_v1();
    let kimi = [KDESC_KIMI_KDA_STEP, KDESC_KIMI_MLA_FUSED, KDESC_KIMI_ROUTER_TOPK, KDESC_KIMI_MOE_COMBINE];
    if kimi.iter().any(|k| reachable.contains(&crate::palw_step::kernel_semantics_id_v1(k))) {
        return Err(PalwAttributableClassErrorV1::KimiKernel);
    }
    if prefill > PALW_J5_INLINE_PROMPT_IDS_V1
        && crate::palw_prompt_ids_v1::palw_prompt_ids_form_of_class_v1(network_form, profile)
            != crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1
    {
        return Err(PalwAttributableClassErrorV1::WidePromptNotMerkle { prefill });
    }
    Ok(())
}

/// **The canonical job `(prefill, decode)` a class is attempted at, as the chain derives it.**
///
/// The base class's is [`crate::palw_base0_profile::PALW_RC_BASE0_CANONICAL`], the job every floor
/// producer and seat runs. A model class's is `(f − 1, 2)` with `f` =
/// `palw_canonical_footprint_floor_v1(n_ctx)` — exactly the held rows' formula
/// (`qwen25_a16_held_canonical_v1`, `qwen36_held_canonical_v1`), which admission pins for every
/// class past `palw_offence_attribution` — and `None` for a context too narrow for it (`f < 2`).
pub fn palw_attempt_canonical_v1(profile: &PalwShapeProfileV3, is_base: bool) -> Option<(u32, u32)> {
    if is_base {
        return Some(crate::palw_base0_profile::PALW_RC_BASE0_CANONICAL);
    }
    let floor = u32::try_from(crate::palw_context_ladder::palw_canonical_footprint_floor_v1(profile.n_ctx)).ok()?;
    (floor >= 2).then_some((floor - 1, 2))
}

/// **The whole context an anchor implies for the canonical job `(p, d)`**, with the prompt's
/// commitment given: `rc_job_context(profile, p, d)` with the anchor's `job_id` and its first 32
/// bytes as the `execution_seed`, the tiled trace scheme where the class commits tiled logits, and
/// the prefill draw's one decode step (`palw_attempt_job_v1(·, true)`).
///
/// On the floor this is `base0_rc_job_v1` of the anchor at the canonical job, then
/// `palw_attempt_job_v1` at the block's draw rule — the job a seat's `verify_material` compares a
/// capture against — byte for byte (golden-tested in `misaka-palw-base0`).
pub fn palw_attempt_context_v1(
    profile: &PalwShapeProfileV3,
    anchor: &Hash64,
    canonical: (u32, u32),
    prompt_hash: Hash64,
) -> PalwJobContextV2 {
    crate::palw_attempt_v2::palw_attempt_job_v1(palw_attempt_canonical_context_v1(profile, anchor, canonical, prompt_hash), true)
}

/// **The canonical job's context before the block's draw** — what a producer's `job_for_anchor`
/// answers under `CoreV1` (callers then apply `palw_attempt_job_v1` at the block's draw rule, as for
/// every family). [`palw_attempt_context_v1`] is this, drawn.
pub fn palw_attempt_canonical_context_v1(
    profile: &PalwShapeProfileV3,
    anchor: &Hash64,
    canonical: (u32, u32),
    prompt_hash: Hash64,
) -> PalwJobContextV2 {
    let mut ctx = crate::palw_base0_profile::rc_job_context(profile, canonical.0, canonical.1);
    ctx.job_id = *anchor;
    ctx.execution_seed = anchor.as_byte_slice()[..32].try_into().expect("a 64-byte hash has 32 bytes");
    if profile.logits_scheme_id == crate::palw_step_refute::tiled_logits_scheme_id_v1() {
        ctx.trace_scheme_id = crate::palw_step_refute::tiled_logits_scheme_id_v1();
    }
    ctx.prompt_token_ids_hash = prompt_hash;
    ctx
}

/// **A producer's `job_for_anchor` under `CoreV1`**: the canonical context (undrawn) and the prompt
/// the anchor names, at `canonical` over the class's `profile` — the same bytes the chain's J5
/// derives. `None` only for a prompt past the prompt-id tree.
pub fn palw_attempt_job_for_anchor_v1(
    profile: &PalwShapeProfileV3,
    anchor: &Hash64,
    canonical: (u32, u32),
    network_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) -> Option<(PalwJobContextV2, Vec<usize>)> {
    let root = palw_attempt_prompt_root_v1(profile, anchor, canonical.0, network_form)?;
    let prompt =
        palw_attempt_prompt_ids_v1(anchor, u64::from(profile.vocab_size), canonical.0).into_iter().map(|id| id as usize).collect();
    Some((palw_attempt_canonical_context_v1(profile, anchor, canonical, root), prompt))
}

/// **The prompt root an anchor implies for a `p`-id prefill**, in the class's form under the
/// network's (`palw_prompt_ids_form_of_class_v1`): the Merkle root for a held class or a Merkle
/// network, the flat digest otherwise. `None` only for a prompt past the prompt-id tree, which no
/// canonical prompt is.
pub fn palw_attempt_prompt_root_v1(
    profile: &PalwShapeProfileV3,
    anchor: &Hash64,
    prefill: u32,
    network_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) -> Option<Hash64> {
    let ids = palw_attempt_prompt_ids_v1(anchor, u64::from(profile.vocab_size), prefill);
    let form = crate::palw_prompt_ids_v1::palw_prompt_ids_form_of_class_v1(network_form, profile);
    crate::palw_prompt_ids_v1::prompt_token_ids_commitment_v1(form, &ids).ok()
}

/// **F1's floor rule, whole** (SPEC §4.3 J5): the context and the prompt a floor attempt anchored at
/// `anchor` must run — [`palw_attempt_context_v1`] over [`palw_attempt_prompt_root_v1`] at the
/// canonical job, at the block's draw rule. The seats' own rule (`verify_material`), in core.
pub fn palw_floor_attempt_context_v1(
    profile: &PalwShapeProfileV3,
    anchor: &Hash64,
    canonical: (u32, u32),
    network_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    prefill_draw: bool,
) -> Option<(PalwJobContextV2, Vec<u32>)> {
    let ids = palw_attempt_prompt_ids_v1(anchor, u64::from(profile.vocab_size), canonical.0);
    let root = palw_attempt_prompt_root_v1(profile, anchor, canonical.0, network_form)?;
    let mut ctx = palw_attempt_context_v1(profile, anchor, canonical, root);
    if !prefill_draw {
        ctx.exact_decode_tokens = canonical.1;
    }
    Some((ctx, ids))
}

/// **A floor binding a test can build without running the floor**: the canonical context the
/// anchor implies, the floor's own checkpoint profile and map, the given roots, and the committed
/// root re-derived as `verify_binding` derives it. Every identity check passes on it by
/// construction, so a test moves ONE field and asks which check notices.
#[cfg(test)]
pub(crate) fn floor_binding_for_tests_v1(
    anchor: &Hash64,
    network_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) -> crate::palw_step_leg::PalwStepBindingV2 {
    use crate::palw_base0_profile::{PALW_RC_BASE0_CANONICAL, PALW_RC_BASE0_GEOMETRY, base0_profile_v1};
    let profile = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("the floor's graph");
    let (job_context, _) =
        palw_floor_attempt_context_v1(&profile, anchor, PALW_RC_BASE0_CANONICAL, network_form, true).expect("a canonical prompt");
    let mut binding = crate::palw_step_leg::PalwStepBindingV2 {
        version: crate::palw_step_leg::PALW_STEP_LEG_OBJECT_VERSION_V1,
        job_context,
        state_chunk_map_id: profile.state_chunk_map_id,
        shape_profile: profile,
        checkpoint_profile: crate::palw_state_chunk_map::integer_kv_checkpoint_profile_v1(1),
        full_logits_trace_root: Hash64::from_u64_word(0x7ACE),
        activation_leg_root: Hash64::default(),
        step_leaf_count: 64,
        step_merkle_root: Hash64::from_u64_word(0x57E9),
        checkpoint_count: 0,
        checkpoint_merkle_root: Hash64::default(),
        committed_execution_root: Hash64::default(),
    };
    binding.activation_leg_root = palw_int_activation_leg_root_v1(&binding.job_context);
    binding.committed_execution_root = crate::palw_step_leg::binding_commitment_root_v1(&binding);
    crate::palw_step_leg::verify_binding_v1(&binding).expect("the fixture binding verifies");
    binding
}

/// [`floor_binding_for_tests_v1`]'s twin for a MODEL class: the floor's graph at `n_ctx`, registered
/// as a class of its own (not the base one), so its canonical job is the model formula and its
/// context is `CoreV1`'s at that job.
#[cfg(test)]
pub(crate) fn model_binding_for_tests_v1(
    anchor: &Hash64,
    n_ctx: u32,
    network_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) -> crate::palw_step_leg::PalwStepBindingV2 {
    use crate::palw_base0_profile::{PALW_RC_BASE0_GEOMETRY, base0_profile_v1};
    let mut profile = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("the floor's graph");
    profile.n_ctx = n_ctx;
    let canonical = palw_attempt_canonical_v1(&profile, false).expect("a context wide enough for the formula");
    let root = palw_attempt_prompt_root_v1(&profile, anchor, canonical.0, network_form).expect("a canonical prompt commits");
    let job_context = palw_attempt_context_v1(&profile, anchor, canonical, root);
    let mut binding = crate::palw_step_leg::PalwStepBindingV2 {
        version: crate::palw_step_leg::PALW_STEP_LEG_OBJECT_VERSION_V1,
        state_chunk_map_id: profile.state_chunk_map_id,
        checkpoint_profile: palw_canonical_checkpoint_profile_v1(&profile),
        activation_leg_root: palw_int_activation_leg_root_v1(&job_context),
        job_context,
        shape_profile: profile,
        full_logits_trace_root: Hash64::from_u64_word(0x7ACE),
        step_leaf_count: 64,
        step_merkle_root: Hash64::from_u64_word(0x57E9),
        checkpoint_count: 0,
        checkpoint_merkle_root: Hash64::default(),
        committed_execution_root: Hash64::default(),
    };
    binding.committed_execution_root = crate::palw_step_leg::binding_commitment_root_v1(&binding);
    crate::palw_step_leg::verify_binding_v1(&binding).expect("the model fixture binding verifies");
    binding
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_base0_profile::{PALW_RC_BASE0_CANONICAL, PALW_RC_BASE0_GEOMETRY, base0_profile_v1};
    use crate::palw_prompt_ids_v1::PalwPromptIdsFormV1;

    /// The loop as `base0_rc_job_v1` spelled it, kept here as the reference the moved function is
    /// held to (the golden test in `misaka-palw-base0` holds the producer's own copy to it too).
    fn reference_loop(anchor: &Hash64, vocab: usize, prefill: u32) -> Vec<u32> {
        let mut prompt = Vec::with_capacity(prefill as usize);
        let mut counter = 0u64;
        while prompt.len() < prefill as usize {
            let mut h = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/base0/rc-job-prompt/v1").to_state();
            h.update(anchor.as_byte_slice());
            h.update(&counter.to_le_bytes());
            let block = h.finalize();
            for word in block.as_bytes().chunks_exact(8) {
                if prompt.len() == prefill as usize {
                    break;
                }
                let v = u64::from_le_bytes(word.try_into().unwrap());
                prompt.push((v % vocab.max(1) as u64) as u32);
            }
            counter += 1;
        }
        prompt
    }

    #[test]
    fn the_moved_prompt_loop_is_the_floors_byte_for_byte() {
        for seed in 0u64..32 {
            let anchor = Hash64::from_u64_word(0xA11C_0000 + seed * 7919);
            for (vocab, count) in [(64usize, 8u32), (151_936, 1_023), (8_292, 17), (1, 3), (0, 5)] {
                assert_eq!(palw_attempt_prompt_ids_v1(&anchor, vocab as u64, count), reference_loop(&anchor, vocab, count));
            }
        }
    }

    #[test]
    fn a_range_is_the_same_ids_as_the_whole_prompt() {
        let anchor = Hash64::from_u64_word(0x5EED);
        let whole = palw_attempt_prompt_ids_v1(&anchor, 151_936, 300);
        for (start, len) in [(0u64, 300u64), (0, 1), (7, 2), (8, 8), (31, 33), (123, 177), (299, 1), (300, 0)] {
            let range = palw_attempt_prompt_ids_range_v1(&anchor, 151_936, start, len);
            assert_eq!(range.as_slice(), &whole[start as usize..(start + len) as usize], "ids {start}..+{len}");
        }
    }

    #[test]
    fn the_floor_context_is_its_canonical_job_at_one_decode_step() {
        let profile = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).unwrap();
        let anchor = Hash64::from_u64_word(0xF100);
        assert_eq!(palw_attempt_canonical_v1(&profile, true), Some(PALW_RC_BASE0_CANONICAL));
        let floor = crate::palw_context_ladder::palw_canonical_footprint_floor_v1(profile.n_ctx) as u32;
        assert_eq!(
            palw_attempt_canonical_v1(&profile, false),
            (floor >= 2).then(|| (floor - 1, 2)),
            "as a class of its own the floor's graph takes the model formula"
        );
        for form in [PalwPromptIdsFormV1::Flat, PalwPromptIdsFormV1::MerkleV1] {
            let (ctx, ids) = palw_floor_attempt_context_v1(&profile, &anchor, PALW_RC_BASE0_CANONICAL, form, true).unwrap();
            assert_eq!((ctx.job_id, &ctx.execution_seed[..]), (anchor, &anchor.as_byte_slice()[..32]));
            assert_eq!((ctx.declared_prefill_tokens, ctx.exact_decode_tokens), (PALW_RC_BASE0_CANONICAL.0, 1));
            assert_eq!(ids.len(), PALW_RC_BASE0_CANONICAL.0 as usize);
            assert_eq!(Some(ctx.prompt_token_ids_hash), palw_attempt_prompt_root_v1(&profile, &anchor, 8, form));
            assert_eq!(ctx.shape_profile_id, profile.shape_profile_id());
            let (full, _) = palw_floor_attempt_context_v1(&profile, &anchor, PALW_RC_BASE0_CANONICAL, form, false).unwrap();
            assert_eq!(full.exact_decode_tokens, PALW_RC_BASE0_CANONICAL.1, "without the draw, the whole canonical job");
        }
    }
    /// **The formula IS the held rows' canonical job**, at every ladder width the families register,
    /// and `None` below a context that can hold it.
    #[test]
    fn the_model_formula_is_the_held_canonical_at_every_width() {
        use crate::palw_qwen25_profile::qwen25_a16_held_canonical_v1;
        use crate::palw_qwen36_profile::qwen36_held_canonical_v1;
        let mut profile = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).unwrap();
        for n_ctx in [16u32, 64, 128, 512, 2_048, 8_192, 262_144, 2_097_152] {
            profile.n_ctx = n_ctx;
            let formula = palw_attempt_canonical_v1(&profile, false);
            assert_eq!(formula, Some(qwen25_a16_held_canonical_v1(n_ctx)), "A16 at {n_ctx}");
            assert_eq!(formula, Some(qwen36_held_canonical_v1(n_ctx)), "Qwen3.6 at {n_ctx}");
        }
        assert_eq!(palw_attempt_canonical_v1(&profile, false), Some((262_143, 2)), "the 2M row's (ADR-0103 Decision 14)");
        profile.n_ctx = 8_192;
        assert_eq!(palw_attempt_canonical_v1(&profile, false), Some((1_023, 2)), "the 8k row's");
        for narrow in [0u32, 1, 8, 15] {
            profile.n_ctx = narrow;
            assert_eq!(palw_attempt_canonical_v1(&profile, false), None, "{narrow}: no room for the formula");
        }
    }

    /// **The moved activation leg and the checkpoint predicate are the producers' own**, and the
    /// rendered rule is the floor's.
    #[test]
    fn the_moved_legs_are_the_producers_rules() {
        let profile = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).unwrap();
        let anchor = Hash64::from_u64_word(0xAC7);
        let (ctx, ids) =
            palw_floor_attempt_context_v1(&profile, &anchor, PALW_RC_BASE0_CANONICAL, PalwPromptIdsFormV1::Flat, true).unwrap();
        // The byte string and the preimage `base0_activation_leg_root_v1` always hashed.
        let mut h = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/base0/activation-leg/v1").to_state();
        h.update(ctx.context_hash().as_byte_slice());
        h.update(&(ctx.declared_prefill_tokens as u64).to_le_bytes());
        h.update(&(ctx.exact_decode_tokens as u64).to_le_bytes());
        h.update(b"no-taps");
        assert_eq!(palw_int_activation_leg_root_v1(&ctx).as_byte_slice(), h.finalize().as_bytes());
        assert_eq!(
            palw_canonical_checkpoint_profile_v1(&profile),
            crate::palw_state_chunk_map::integer_kv_checkpoint_profile_v1(1),
            "a class with no recurrence checkpoints every call"
        );
        let mut hybrid = profile.clone();
        hybrid.gdn_heads = 2;
        assert_eq!(
            palw_canonical_checkpoint_profile_v1(&hybrid),
            crate::palw_state_chunk_map::integer_kv_checkpoint_profile_v1(hybrid.n_ctx),
            "a hybrid at its window"
        );
        assert_eq!(
            palw_attempt_output_root_v1(&ctx, &ids),
            crate::palw_v2::output_commitment_v2(&ctx.context_hash(), &ids, &crate::palw_v2::rendered_output_hash_v2(&[]))
        );
    }

    /// **T18w (addendum §4-bis.8): a registration past the fence must be attributable.** The real
    /// rows pass (A16 graph-v7 at 8,192 and 2,097,152, Qwen3.6 graph-v7 at 512, at the formula's
    /// job); a canonical that is not the formula, a class too narrow for it, a Float32 class (no
    /// provable head), a Kimi K3 class and a wide prompt committed flat are each refused by name.
    #[test]
    fn t18w_a_registration_is_attributable_or_refused_by_name() {
        use crate::palw_base0_profile::rc_job_context;
        use crate::palw_context_ladder::{palw_a16_context_row_profile_v7, palw_qwen36_context_row_profile_v7};
        use PalwAttributableClassErrorV1 as R;
        let merkle = PalwPromptIdsFormV1::MerkleV1;
        for (label, profile) in [
            ("A16@8192", palw_a16_context_row_profile_v7(8_192).unwrap()),
            ("A16@2M", palw_a16_context_row_profile_v7(2_097_152).unwrap()),
            ("Q36@512", palw_qwen36_context_row_profile_v7(512).unwrap()),
        ] {
            let (p, d) = palw_attempt_canonical_v1(&profile, false).unwrap();
            assert_eq!(palw_attributable_class_v1(&profile, &rc_job_context(&profile, p, d), false, merkle), Ok(()), "{label}");
            assert_eq!(
                palw_attributable_class_v1(&profile, &rc_job_context(&profile, p - 1, d), false, merkle),
                Err(R::CanonicalNotTheFormula { got_prefill: p - 1, got_decode: d, want_prefill: p, want_decode: d }),
                "{label}: a canonical the registrant chose"
            );
            let mut float = profile.clone();
            float.lane = crate::palw_step::PalwStepLaneV1::Float32;
            assert_eq!(
                palw_attributable_class_v1(&float, &rc_job_context(&float, p, d), false, merkle),
                Err(R::HeadUnproven),
                "{label}"
            );
        }
        // Too narrow: n_ctx 8 has no formula job, whatever it registers.
        let mut narrow = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).unwrap();
        narrow.n_ctx = 8;
        assert_eq!(palw_attributable_class_v1(&narrow, &rc_job_context(&narrow, 4, 2), false, merkle), Err(R::TooNarrow));
        // The floor as a genesis base row passes on its own canonical job.
        let floor = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).unwrap();
        assert_eq!(palw_attributable_class_v1(&floor, &rc_job_context(&floor, 8, 4), true, merkle), Ok(()));
        // Kimi K3.
        let mut kimi = crate::palw_kimi_k3_profile::kimi_k3_profile_v1(crate::palw_kimi_k3_profile::KIMI_K3_CARD).unwrap();
        assert_eq!(
            palw_attributable_class_v1(&kimi, &rc_job_context(&kimi, 8, 2), false, merkle),
            Err(R::TooNarrow),
            "the shipped Kimi card registers n_ctx 10: too narrow"
        );
        // At a width the formula admits, its graph is refused on its own.
        kimi.n_ctx = 128;
        let (p, d) = palw_attempt_canonical_v1(&kimi, false).expect("wide enough for the formula");
        let verdict = palw_attributable_class_v1(&kimi, &rc_job_context(&kimi, p, d), false, merkle);
        assert!(matches!(verdict, Err(R::KimiKernel) | Err(R::HeadUnproven)), "a Kimi K3 class is refused: {verdict:?}");
        let mut kimi_with_head = kimi.clone();
        kimi_with_head.post_nodes = floor.post_nodes.clone();
        kimi_with_head.vocab_size = floor.vocab_size;
        kimi_with_head.logits_scheme_id = floor.logits_scheme_id;
        kimi_with_head.lane = floor.lane;
        assert!(crate::palw_step::palw_logits_head_v1(&kimi_with_head).is_some(), "the grafted head is provable");
        assert_eq!(
            palw_attributable_class_v1(&kimi_with_head, &rc_job_context(&kimi_with_head, p, d), false, merkle),
            Err(R::KimiKernel),
            "a Kimi kernel anywhere in the graph, whatever its head"
        );
        // A wide prompt committed flat: the floor's graph at 2M width, not held, on a Flat network.
        let mut wide = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).unwrap();
        wide.n_ctx = 2_097_152;
        let (p, d) = palw_attempt_canonical_v1(&wide, false).unwrap();
        assert_eq!(
            palw_attributable_class_v1(&wide, &rc_job_context(&wide, p, d), false, PalwPromptIdsFormV1::Flat),
            Err(R::WidePromptNotMerkle { prefill: p })
        );
        assert_eq!(palw_attributable_class_v1(&wide, &rc_job_context(&wide, p, d), false, merkle), Ok(()), "and Merkle opens it");
    }

    /// **testnet-12's genesis rows are attributable** (addendum §4-bis.8: "genesis rows already
    /// satisfy (a)–(d); a test asserts it"): every row the genesis registers with a carriage passes
    /// under the network's form, the floor on its own canonical job.
    #[test]
    fn t18w_testnet_12s_genesis_rows_are_attributable() {
        let params = crate::config::params::palw_t12_shipped_params();
        let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
            panic!("testnet-12 is a V2 network")
        };
        let form = params.palw_prompt_ids_form_v1();
        let mut rows = 0;
        for object in &bundle.genesis_objects {
            if let crate::palw_state_v2::PalwConsensusObjectV2::ClassRegistered { class_id, admission: Some(carriage), .. } = object {
                let is_base = *class_id == bundle.base_class_id;
                assert_eq!(
                    palw_attributable_class_v1(&carriage.profile, &carriage.canonical, is_base, form),
                    Ok(()),
                    "genesis class {class_id} (base: {is_base})"
                );
                rows += 1;
            }
        }
        assert!(rows >= 2, "testnet-12 registers its model rows with carriages ({rows})");
    }
}
