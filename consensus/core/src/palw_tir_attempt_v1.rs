//! **RFC-0002 Phase F, step F6: an IR class's attempt job** — its canonical job, its yardstick
//! context and the job an anchor names (SPEC §4.3 J5), as the chain derives them for an IR class
//! (`docs/design/palw/tir/phase-f-integration.md` §2.4 step 6, §2.10).
//!
//! The legacy rules (`palw_attempt_rules_v1`) read a shape profile, and an IR class has none. Each
//! value here is the legacy one with the profile's facts replaced by the class's, and nothing else
//! changed:
//!
//! * **The canonical job** is the model formula `(f − 1, 2)` with
//!   `f = palw_canonical_footprint_floor_v1(layout.max_context)` — `None` when `f < 2`. An IR class
//!   is never the base class, so the floor's own canonical job never applies.
//! * **The yardstick context** is `rc_job_context`'s: `network_id = "misaka-palw-rc"`, every identity
//!   field zero (the tokenizer too: the class id already commits it), `shape_profile_id` = the IR
//!   class id, `max_context_tokens = layout.max_context`, and the tiled trace scheme when the
//!   program commits tiled logits (`trace_scheme_id_v2()` otherwise — the only two a job context's
//!   shape check admits).
//! * **The prompt** an anchor names is `palw_attempt_prompt_ids_v1(anchor, token_bound, prefill)`,
//!   committed in the class's form: the Merkle root for a program of the held history bound
//!   (`HISTORY_BOUND_V1_HELD`, the held regime, ADR-0118 Decision 3), the network's form otherwise.
//! * **The attempt context** is the yardstick at the canonical job with the anchor as `job_id`, its
//!   first 32 bytes as `execution_seed` and the prompt's commitment, drawn by
//!   `palw_attempt_job_v1(·, true)` — J5's expected context, compared by `context_hash`.
//!
//! [`PalwTirJobFactsV1`] is everything these are a function of. The fold keeps it in the class's
//! `tir_classes` row, so J5 never decodes a program; the class-taking wrappers decode the program
//! once per call and are meant for tools and tests (a node caches the facts).

use crate::Hash64;
use crate::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use crate::palw_tir_class_v1::PalwTirClassV1;
use crate::palw_v2::PalwJobContextV2;
use misaka_palw_tir::TirProgramV1;

/// **What an IR class's attempt jobs are a function of** — read from the class and its program
/// once, stored in the class's `tir_classes` row (F6), and compared, never believed: every field is
/// derived from the carried class by [`PalwTirJobFactsV1::of`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwTirJobFactsV1 {
    /// `tir_class_id_v1(class, artifact_root)` — the job context's `shape_profile_id`.
    pub class_id: Hash64,
    /// `layout.max_context`: the most positions a job may touch, and the context's
    /// `max_context_tokens`.
    pub max_context: u32,
    /// The program's `token_bound`: prompt ids are drawn in `[0, token_bound)`.
    pub token_bound: u32,
    /// The program commits tiled logits (`logits_scheme_id == tiled_logits_scheme_id_v1()`).
    pub tiled: bool,
    /// The program's `history_bound` is the held one (`HISTORY_BOUND_V1_HELD`).
    pub held: bool,
}

impl PalwTirJobFactsV1 {
    /// The facts of `class` whose decoded program is `program`, under `class_id`.
    pub fn of(class: &PalwTirClassV1, program: &TirProgramV1, class_id: Hash64) -> Self {
        Self {
            class_id,
            max_context: class.layout.max_context,
            token_bound: program.token_bound,
            tiled: Hash64::from_bytes(program.logits_scheme_id) == crate::palw_step_refute::tiled_logits_scheme_id_v1(),
            held: program.history_bound == misaka_palw_tir::program::HISTORY_BOUND_V1_HELD,
        }
    }

    /// [`Self::of`], decoding the class's program strictly first.
    pub fn of_class(class: &PalwTirClassV1, class_id: Hash64) -> misaka_palw_tir::TirResult<Self> {
        let program = class.decode_program()?;
        Ok(Self::of(class, &program, class_id))
    }

    /// The prompt-ids form the class's jobs commit in under the network's `network` form: the
    /// Merkle root for a held program on every network, the network's form otherwise.
    pub fn prompt_ids_form(&self, network: PalwPromptIdsFormV1) -> PalwPromptIdsFormV1 {
        if self.held { PalwPromptIdsFormV1::MerkleV1 } else { network }
    }
}

/// **The canonical attempt job `(prefill, decode)` of a class whose layout allows `max_context`
/// positions**: `(f − 1, 2)`, `f = palw_canonical_footprint_floor_v1(max_context)`, or `None` when
/// the context is too narrow for the formula (`f < 2`). The legacy model formula
/// (`palw_attempt_canonical_v1(profile, false)`), over the layout's `max_context`.
pub fn palw_tir_attempt_canonical_of_v1(max_context: u32) -> Option<(u32, u32)> {
    let floor = u32::try_from(crate::palw_context_ladder::palw_canonical_footprint_floor_v1(max_context)).ok()?;
    (floor >= 2).then_some((floor - 1, 2))
}

/// [`palw_tir_attempt_canonical_of_v1`] of the class's layout.
pub fn palw_tir_attempt_canonical_v1(class: &PalwTirClassV1) -> Option<(u32, u32)> {
    palw_tir_attempt_canonical_of_v1(class.layout.max_context)
}

/// **The yardstick context of an IR class at `(prefill, decode)`** — `rc_job_context`'s twin: the
/// same fixed identity fields, the class id in `shape_profile_id`, the layout's `max_context`, the
/// class's trace scheme. What a registration carries as its canonical job, and what an attempt
/// context is built on.
pub fn palw_tir_job_context_v1(facts: &PalwTirJobFactsV1, canonical: (u32, u32)) -> PalwJobContextV2 {
    PalwJobContextV2 {
        version: crate::palw_v2::PALW_TRACE_COMMITMENT_VERSION_V2,
        network_id: b"misaka-palw-rc".to_vec(),
        job_id: Hash64::default(),
        job_nullifier: Hash64::default(),
        assignment_id: Hash64::default(),
        execution_seed: [0; 32],
        model_profile_id: Hash64::default(),
        runtime_manifest_hash: Hash64::default(),
        runtime_class_id: Hash64::default(),
        shape_profile_id: facts.class_id,
        trace_scheme_id: if facts.tiled {
            crate::palw_step_refute::tiled_logits_scheme_id_v1()
        } else {
            crate::palw_v2::trace_scheme_id_v2()
        },
        cu_ruleset_id: Hash64::default(),
        tokenizer_id: Hash64::default(),
        prompt_token_ids_hash: Hash64::default(),
        declared_prefill_tokens: canonical.0,
        exact_decode_tokens: canonical.1,
        max_context_tokens: facts.max_context,
    }
}

/// [`palw_tir_job_context_v1`] of `class` under `class_id` (decodes the program once).
/// `None` when the program does not decode.
pub fn palw_tir_canonical_context_v1(class: &PalwTirClassV1, class_id: Hash64, canonical: (u32, u32)) -> Option<PalwJobContextV2> {
    let facts = PalwTirJobFactsV1::of_class(class, class_id).ok()?;
    Some(palw_tir_job_context_v1(&facts, canonical))
}

/// **The prompt root an anchor implies for a `prefill`-id prompt** of the class, in the class's
/// form under the network's. `None` only for a prompt past the prompt-id tree.
pub fn palw_tir_attempt_prompt_root_v1(
    facts: &PalwTirJobFactsV1,
    anchor: &Hash64,
    prefill: u32,
    network_form: PalwPromptIdsFormV1,
) -> Option<Hash64> {
    let ids = crate::palw_attempt_rules_v1::palw_attempt_prompt_ids_v1(anchor, u64::from(facts.token_bound), prefill);
    crate::palw_prompt_ids_v1::prompt_token_ids_commitment_v1(facts.prompt_ids_form(network_form), &ids).ok()
}

/// **The canonical job's context before the block's draw**, for the anchor and the prompt's
/// commitment: the yardstick with the anchor as `job_id`, its first 32 bytes as the seed, and
/// `prompt_hash` — what a producer's `job_for_anchor` answers.
pub fn palw_tir_attempt_canonical_context_v1(
    facts: &PalwTirJobFactsV1,
    anchor: &Hash64,
    canonical: (u32, u32),
    prompt_hash: Hash64,
) -> PalwJobContextV2 {
    let mut ctx = palw_tir_job_context_v1(facts, canonical);
    ctx.job_id = *anchor;
    ctx.execution_seed = anchor.as_byte_slice()[..32].try_into().expect("a 64-byte hash has 32 bytes");
    ctx.prompt_token_ids_hash = prompt_hash;
    ctx
}

/// **J5's expected context**: [`palw_tir_attempt_canonical_context_v1`], drawn at the prefill draw
/// (`palw_attempt_job_v1(·, true)`: one decode step), exactly as the legacy
/// `palw_attempt_context_v1`.
pub fn palw_tir_attempt_context_v1(
    facts: &PalwTirJobFactsV1,
    anchor: &Hash64,
    canonical: (u32, u32),
    prompt_hash: Hash64,
) -> PalwJobContextV2 {
    crate::palw_attempt_v2::palw_attempt_job_v1(palw_tir_attempt_canonical_context_v1(facts, anchor, canonical, prompt_hash), true)
}

/// **A producer's `job_for_anchor` for an IR class**: the canonical context (undrawn) and the
/// prompt the anchor names, at `canonical` — the same bytes the chain's J5 derives. `None` only for
/// a prompt past the prompt-id tree.
pub fn palw_tir_attempt_job_for_anchor_of_v1(
    facts: &PalwTirJobFactsV1,
    anchor: &Hash64,
    canonical: (u32, u32),
    network_form: PalwPromptIdsFormV1,
) -> Option<(PalwJobContextV2, Vec<u32>)> {
    let ids = crate::palw_attempt_rules_v1::palw_attempt_prompt_ids_v1(anchor, u64::from(facts.token_bound), canonical.0);
    let root = crate::palw_prompt_ids_v1::prompt_token_ids_commitment_v1(facts.prompt_ids_form(network_form), &ids).ok()?;
    Some((palw_tir_attempt_canonical_context_v1(facts, anchor, canonical, root), ids))
}

/// [`palw_tir_attempt_job_for_anchor_of_v1`] of `class` under `class_id` (decodes the program
/// once). `None` when the program does not decode or the prompt is past the prompt-id tree.
pub fn palw_tir_attempt_job_for_anchor_v1(
    class: &PalwTirClassV1,
    class_id: Hash64,
    canonical: (u32, u32),
    anchor: &Hash64,
    network_form: PalwPromptIdsFormV1,
) -> Option<(PalwJobContextV2, Vec<u32>)> {
    let facts = PalwTirJobFactsV1::of_class(class, class_id).ok()?;
    palw_tir_attempt_job_for_anchor_of_v1(&facts, anchor, canonical, network_form)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_attempt_rules_v1::{palw_attempt_canonical_v1, palw_attempt_context_v1, palw_attempt_job_for_anchor_v1};

    fn anchor(b: u8) -> Hash64 {
        Hash64::from_bytes([b; 64])
    }

    /// **The IR rules are the legacy rules with the profile's facts replaced.** For a legacy
    /// profile, the facts it would have as an IR class (its id, `n_ctx`, vocabulary, scheme, held
    /// map) derive the same canonical job, the same contexts and the same prompt, byte for byte.
    #[test]
    fn the_ir_rules_are_the_legacy_rules_over_the_same_facts() {
        use crate::palw_qwen25_profile::{QWEN25_1_5B, qwen25_a16_profile_v7};
        let profile = qwen25_a16_profile_v7(QWEN25_1_5B).expect("the graph-v7 profile");
        let facts = PalwTirJobFactsV1 {
            class_id: profile.shape_profile_id(),
            max_context: profile.n_ctx,
            token_bound: profile.vocab_size,
            tiled: profile.logits_scheme_id == crate::palw_step_refute::tiled_logits_scheme_id_v1(),
            held: crate::palw_state_chunk_map::palw_profile_is_held_v4(&profile),
        };
        let canonical = palw_attempt_canonical_v1(&profile, false).expect("wide enough");
        assert_eq!(palw_tir_attempt_canonical_of_v1(facts.max_context), Some(canonical));
        for form in [PalwPromptIdsFormV1::Flat, PalwPromptIdsFormV1::MerkleV1] {
            for a in [anchor(1), anchor(0xA7)] {
                let (legacy_ctx, legacy_ids) = palw_attempt_job_for_anchor_v1(&profile, &a, canonical, form).expect("a prompt");
                let (ir_ctx, ir_ids) = palw_tir_attempt_job_for_anchor_of_v1(&facts, &a, canonical, form).expect("a prompt");
                assert_eq!(ir_ctx, legacy_ctx, "the undrawn context");
                assert_eq!(ir_ids.iter().map(|x| *x as usize).collect::<Vec<_>>(), legacy_ids, "the prompt");
                let root = ir_ctx.prompt_token_ids_hash;
                assert_eq!(palw_tir_attempt_prompt_root_v1(&facts, &a, canonical.0, form), Some(root));
                assert_eq!(
                    palw_tir_attempt_context_v1(&facts, &a, canonical, root).context_hash(),
                    palw_attempt_context_v1(&profile, &a, canonical, root).context_hash(),
                    "J5's expected context"
                );
            }
        }
    }

    #[test]
    fn the_formula_and_its_narrow_edge() {
        assert_eq!(palw_tir_attempt_canonical_of_v1(8_192), Some((1_023, 2)));
        assert_eq!(palw_tir_attempt_canonical_of_v1(16), Some((1, 2)));
        for narrow in [0, 1, 8, 15] {
            assert_eq!(palw_tir_attempt_canonical_of_v1(narrow), None, "{narrow}");
        }
    }

    /// The facts of a real IR class: the yardstick passes the job-context shape check, names the
    /// class, and carries the scheme its program commits; a held program commits Merkle prompts on
    /// a flat network.
    #[test]
    fn a_class_s_facts_and_contexts() {
        let x = crate::palw_tir_court_v1::test_support::tiny_execution(None);
        // The court's tiny class touches four positions, too narrow for the formula; the same
        // program under a 64-position layout is a class with a canonical job.
        assert_eq!(palw_tir_attempt_canonical_v1(&x.binding.class), None, "four positions: no canonical job");
        let mut wide = x.binding.class.clone();
        wide.layout.max_context = 64;
        let class = &wide;
        let class_id = class.class_id(&x.binding.artifact_root);
        let facts = PalwTirJobFactsV1::of_class(class, class_id).expect("the class decodes");
        let program = class.decode_program().unwrap();
        assert_eq!(facts.token_bound, program.token_bound);
        assert!(facts.tiled, "the tiny class commits tiled logits");
        assert!(!facts.held);
        let canonical = palw_tir_attempt_canonical_v1(class).expect("64 positions: (7, 2)");
        assert_eq!(canonical, (7, 2));
        let ctx = palw_tir_canonical_context_v1(class, class_id, canonical).expect("decodes");
        crate::palw_slash::check_job_context_shape(&ctx).expect("the yardstick is a well-formed context");
        assert_eq!(ctx.shape_profile_id, class_id);
        assert_eq!(ctx.trace_scheme_id, crate::palw_step_refute::tiled_logits_scheme_id_v1());
        let (job, ids) =
            palw_tir_attempt_job_for_anchor_v1(class, class_id, canonical, &anchor(3), PalwPromptIdsFormV1::Flat).expect("a job");
        assert_eq!(ids.len() as u32, canonical.0);
        assert!(ids.iter().all(|id| *id < program.token_bound));
        let held = PalwTirJobFactsV1 { held: true, ..facts };
        assert_eq!(held.prompt_ids_form(PalwPromptIdsFormV1::Flat), PalwPromptIdsFormV1::MerkleV1);
        assert_eq!(facts.prompt_ids_form(PalwPromptIdsFormV1::Flat), PalwPromptIdsFormV1::Flat);
        assert_eq!(job.job_id, anchor(3));
        assert_eq!(
            palw_tir_attempt_context_v1(&facts, &anchor(3), canonical, job.prompt_token_ids_hash),
            crate::palw_attempt_v2::palw_attempt_job_v1(job, true)
        );
    }
}
