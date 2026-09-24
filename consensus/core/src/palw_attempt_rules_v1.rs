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
//! **F1 (this stage) covers the floor.** A model class's canonical job is still its registrant's
//! and its context still carries artifact-held fields (`CoreV1`, F1-M, replaces both), so
//! [`palw_attempt_canonical_v1`] answers `None` for every class but the base one and J5 does not
//! run there — the relabel on a model class is F1-M's residual until then.

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

/// **The canonical job `(prefill, decode)` a class is attempted at, as the chain derives it** —
/// `None` where the chain does not (yet) derive one.
///
/// The base class's is [`crate::palw_base0_profile::PALW_RC_BASE0_CANONICAL`], the job every floor
/// producer and seat runs. F1 stops there: a model class's canonical job is still carried by its
/// registration and its context by its artifact, so its whole-context rule is F1-M's (`CoreV1`).
pub fn palw_attempt_canonical_v1(_profile: &PalwShapeProfileV3, is_base: bool) -> Option<(u32, u32)> {
    is_base.then_some(crate::palw_base0_profile::PALW_RC_BASE0_CANONICAL)
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
    let mut ctx = crate::palw_base0_profile::rc_job_context(profile, canonical.0, canonical.1);
    ctx.job_id = *anchor;
    ctx.execution_seed = anchor.as_byte_slice()[..32].try_into().expect("a 64-byte hash has 32 bytes");
    if profile.logits_scheme_id == crate::palw_step_refute::tiled_logits_scheme_id_v1() {
        ctx.trace_scheme_id = crate::palw_step_refute::tiled_logits_scheme_id_v1();
    }
    ctx.prompt_token_ids_hash = prompt_hash;
    crate::palw_attempt_v2::palw_attempt_job_v1(ctx, true)
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
        activation_leg_root: Hash64::from_u64_word(0xAC71),
        step_leaf_count: 64,
        step_merkle_root: Hash64::from_u64_word(0x57E9),
        checkpoint_count: 0,
        checkpoint_merkle_root: Hash64::default(),
        committed_execution_root: Hash64::default(),
    };
    binding.committed_execution_root = crate::palw_step_leg::binding_commitment_root_v1(&binding);
    crate::palw_step_leg::verify_binding_v1(&binding).expect("the fixture binding verifies");
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
        assert_eq!(palw_attempt_canonical_v1(&profile, false), None, "F1 derives the floor's job alone");
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
}
