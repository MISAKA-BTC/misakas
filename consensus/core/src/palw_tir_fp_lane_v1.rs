//! **RFC-0002 Phase H: the Free Prompt lane of an IR (PALW-TIR) class** (design: `docs/design/palw/tir/phase-f-integration.md` §6).
//!
//! Below `palw_tir_public_conviction_v1` an IR class has no free-prompt lane: it has no `fp_work_profiles` row (that row is a
//! legacy `PalwShapeProfileV3`), so past `palw_fp_derived_work` every IR commitment is refused by name
//! (`FreePromptClassHasNoWorkProfile`) and the extraction walk skips it (`Unpublished`). Nothing here runs there.
//!
//! **Past that fence the lane is open, and only through the TIR public frame.** The fence that opens it is the frame's own: every
//! IR commitment it admits must carry its transaction-paired `TFP1` ORIGINAL frame (G-03, `require_original_frame_v1`), so no IR
//! free-prompt claim ever exists without the one public conviction door the G14 claim rests on. The lane adds three rules and no
//! state:
//!
//! 1. **The work is derived from the admitted class, not published.** The legacy lane needed a second row because its class state
//!    held only an id; an IR class's state holds its admitted program (`tir_classes`, checked against `graph_ir_root`) and the
//!    layout the frame already requires (`TirLayout`, authenticated against `layout_digest`). The run's step leaves are the step
//!    space's closed form over the committed counts (`PalwTirStepSpaceV1::running_total`, the enumeration the frame's source and
//!    the court walk), and the credited compute is the structural work vector over the NEW positions
//!    (`PalwTirWorkShapeV1::work_v1`, the walk the attempt lane's IR weight and the registry's work come from). No constant enters:
//!    the ceilings are the class's step ladder and the layout's `max_context`.
//! 2. **The lane is authenticated by coverage, as the IR attempt lane is.** A class whose every reachable primitive is covered by
//!    ONE family the chain certified (for either lane) is adjudicable by the generic interpreter court; the free-prompt-specific
//!    doors (selection under the job's own rule, output identity, stop rule, DA) are the frame's, in consensus code, the same for
//!    every class. No per-class drill, no model-level review (PRINCIPLES §9: claim-level verification only).
//! 3. **Compute only (ADR-0148).** An IR class has no leaves-era price; a ruleset whose canonical-work unit is not in force refuses
//!    its commitments by name. Every network that can arm the frame prices compute (testnet-12 from genesis).
//!
//! Out of scope by decision: seeded Float32 (the frame refuses a Float32 view), images / vision / MTP (genesis models are text-only),
//! a consumed prefix STATE (a staged semantic stage; G-18), constrained jobs without a semantic contract (the frame refuses them).
//! No cryptographic proof of computation is used or contemplated (PRINCIPLES §8): hashes and Merkle roots here authenticate data.
use crate::palw_freeprompt_v3::{PalwFpDerivedWorkV1, PalwFpExecutionModeV1, PalwFpPrefixStateV1};
use crate::palw_tir_admission_v1::PalwTirClassRecordV1;
use crate::palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PalwTirClassV1, PalwTirLayoutV1};
use crate::palw_tir_step_v1::{PalwTirJobShapeV1, PalwTirStepSpaceV1};

/// **The IR class an FP commitment is priced against**, rebuilt from its two authenticated rows: the admitted record's program and
/// tokenizer and the published layout. Refused unless the layout is the one the class id binds (`layout_digest`) and the program is
/// the record's (`graph_ir_root`, the load check).
pub fn palw_tir_fp_class_v1(record: &PalwTirClassRecordV1, layout: &PalwTirLayoutV1) -> Result<PalwTirClassV1, String> {
    record.check_program_v1().map_err(str::to_string)?;
    let class = PalwTirClassV1 {
        version: PALW_TIR_CLASS_VERSION_V1,
        program: record.program.as_ref().clone(),
        layout: layout.clone(),
        tokenizer_id: record.tokenizer_id,
    };
    if class.layout_digest() != record.layout_digest || class.graph_ir_root() != record.graph_ir_root {
        return Err("the published layout or program does not authenticate the registered IR class".into());
    }
    Ok(class)
}

/// The job shape of a run of `prompt_tokens` prompt ids that emitted `decode_tokens_executed` tokens — the positions it touched
/// (`P + D − 1`), refused past the layout's `max_context` exactly as `PalwTirStepSpaceV1::job_shape` refuses a context.
fn job_shape_v1(space: &PalwTirStepSpaceV1, prompt_tokens: u32, decode_tokens_executed: u32) -> Result<PalwTirJobShapeV1, String> {
    if prompt_tokens == 0 {
        return Err("an IR job has at least one prompt position".into());
    }
    if decode_tokens_executed == 0 {
        return Err("an IR job emits at least one token".into());
    }
    let positions = prompt_tokens.checked_add(decode_tokens_executed - 1).ok_or("the job's positions overflow")?;
    if positions > space.layout.max_context {
        return Err(format!("the run touches {positions} positions, past the class's max_context {}", space.layout.max_context));
    }
    Ok(PalwTirJobShapeV1 { prefill: prompt_tokens, positions })
}

/// **The run's step leaves** — the closed form the frame's source and the court enumerate (`running_total` over every position the
/// run touched), refused past `cap` (the class's step ladder). What a commitment's `work_leaves` must equal.
pub fn palw_tir_fp_leaf_count_v1(
    space: &PalwTirStepSpaceV1,
    prompt_tokens: u32,
    decode_tokens_executed: u32,
    cap: u64,
) -> Result<u64, String> {
    let job = job_shape_v1(space, prompt_tokens, decode_tokens_executed)?;
    let total = space.running_total(&job, job.positions);
    if total > u128::from(cap) {
        return Err(format!("the run's {total} step leaves are past the class's ladder {cap}"));
    }
    Ok(total as u64)
}

/// The leaves of the first `k ≤ P` prompt positions' PREFILL: every occurrence but `post` over `[0, k)`, with the checkpoint and
/// history leaves those positions close. `post` runs only at `a ≥ P − 1` and is the first token's logits, which the compute credit
/// counts as new work (`work_v1` runs `post` from `P − 1` whatever was reused) — so it is not prefill here either.
fn prefill_leaves_v1(space: &PalwTirStepSpaceV1, job: &PalwTirJobShapeV1, k: u32) -> u128 {
    let k = k.min(job.prefill);
    let with_post = space.running_total(job, k);
    if k < job.prefill {
        return with_post;
    }
    // Position `P − 1` is the only prefill position that runs `post`; the same position in a job whose prompt is one longer does not.
    let at = job.prefill - 1;
    let without = PalwTirJobShapeV1 { prefill: job.prefill + 1, positions: job.positions.max(job.prefill) };
    let post_at = space.leaves_at_position(job, at).saturating_sub(space.leaves_at_position(&without, at));
    with_post.saturating_sub(u128::from(post_at))
}

/// **An IR free-prompt run's derived work**: its leaves (what `work_leaves` must equal), its credited leaves (the NEW positions'),
/// and its credited compute — the structural work vector of the NEW positions in the attempt lane's provisional scalar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwTirFpWorkV1 {
    pub derived: PalwFpDerivedWorkV1,
    pub credited_compute: u128,
}

/// **Derive an IR free-prompt run's work from the admitted class and the run's two committed counts** (ADR-0145 I1 over the IR
/// class). `accounted_prefix_tokens` is the chain's reading of the prompt this class was already paid to evaluate (under the decode
/// rules, the whole prompt); a consumed prefix STATE is refused (v1: the TIR prefix-state stage is a staged semantic court, G-18).
/// `min_select_arms`: the second IR fence's `Select`-arm credit (`palw_tir_fence2`), as the registry's work reads it.
pub fn palw_tir_fp_derive_work_v1(
    class: &PalwTirClassV1,
    prompt_tokens: u32,
    decode_tokens_executed: u32,
    accounted_prefix_tokens: u32,
    prefix_state: PalwFpPrefixStateV1,
    cap: u64,
    min_select_arms: bool,
) -> Result<PalwTirFpWorkV1, String> {
    if !prefix_state.is_genesis() {
        return Err("an IR free-prompt claim consumes no prefix state in v1 (the TIR prefix-state stage is a staged court)".into());
    }
    let space = PalwTirStepSpaceV1::new(class).map_err(|e| e.to_string())?;
    let total_leaves = palw_tir_fp_leaf_count_v1(&space, prompt_tokens, decode_tokens_executed, cap)?;
    let job = job_shape_v1(&space, prompt_tokens, decode_tokens_executed)?;
    let prefix = accounted_prefix_tokens.min(prompt_tokens);
    let mode = if prefix > 0 { PalwFpExecutionModeV1::PrefixReused } else { PalwFpExecutionModeV1::Uncached };
    let credited_leaves = total_leaves.saturating_sub(prefill_leaves_v1(&space, &job, prefix).min(u128::from(u64::MAX)) as u64);
    let facts = crate::palw_canonical_work_v1::PalwCanonicalExecutionFactsV1 {
        prefill_tokens: prompt_tokens,
        generated_tokens: decode_tokens_executed,
        reused_prefix_tokens: prefix,
        mode: if prefix > 0 {
            crate::palw_canonical_work_v1::PalwExecutionModeV1::PrefixReused
        } else {
            crate::palw_canonical_work_v1::PalwExecutionModeV1::Uncached
        },
    };
    let credited_compute = crate::palw_tir_work_v1::palw_tir_work_shape_with_v1(&space.program, min_select_arms)
        .map_err(|e| e.to_string())?
        .work_v1(&facts)
        .map_err(|e| e.to_string())?
        .provisional_scalar_v1();
    Ok(PalwTirFpWorkV1 {
        derived: PalwFpDerivedWorkV1 { total_leaves, credited_leaves, accounted_prefix_tokens: prefix, mode },
        credited_compute,
    })
}

/// **Is the IR class's free-prompt lane authenticated?** Every primitive its program reaches is covered by ONE certified family
/// (the IR attempt lane's rule, `ClassLaneCertifiedTirV1`; containment is by one family, never a union). `families` is every family
/// the chain certified, for either lane.
pub fn palw_tir_fp_lane_covered_v1<'a>(
    record: &PalwTirClassRecordV1,
    families: impl IntoIterator<Item = &'a crate::palw_e2e_adjudicability::PalwE2eFamilyV1>,
) -> Result<(), String> {
    record.check_program_v1().map_err(str::to_string)?;
    let program = misaka_palw_tir::TirProgramV1::decode_canonical(&record.program).map_err(|e| e.to_string())?;
    let reachable = crate::palw_tir_admission_v1::palw_tir_reachable_prims_v1(&program);
    if families.into_iter().any(|family| reachable.is_subset(&family.kernel_ids)) {
        Ok(())
    } else {
        Err("no certified family covers every primitive the IR class's program reaches".into())
    }
}

/// The refusal text of an IR commitment priced outside the compute era.
pub const PALW_TIR_FP_COMPUTE_ONLY_V1: &str =
    "an IR class's free-prompt lane prices compute (ADR-0148): below the canonical-work fence it has no price";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Hash64;

    fn tiny() -> (PalwTirClassRecordV1, PalwTirLayoutV1, crate::palw_tir_court_v1::test_support::TinyExecution) {
        let x = crate::palw_tir_court_v1::test_support::tiny_execution(None);
        let (record, _) = crate::palw_tir_admission_v1::palw_tir_class_record_v1(&x.binding.class, &x.binding.artifact_root).unwrap();
        (record, x.binding.class.layout.clone(), x)
    }

    /// The derived leaves are the leaves the executed capture committed — the frame's `work_leaves` check passes an honest run.
    #[test]
    fn tir_fp_leaves_are_the_captures_own_and_the_credit_is_the_new_positions() {
        let (record, layout, x) = tiny();
        let class = palw_tir_fp_class_v1(&record, &layout).unwrap();
        let (p, d) = (x.prompt.len() as u32, x.generated.len() as u32);
        let w = palw_tir_fp_derive_work_v1(&class, p, d, 0, PalwFpPrefixStateV1::genesis(Hash64::default()), 1 << 40, false).unwrap();
        assert_eq!(w.derived.total_leaves, x.binding.step_leaf_count, "the closed form is the capture's count");
        assert_eq!(w.derived.credited_leaves, w.derived.total_leaves, "nothing paid: everything is new");
        // Under the decode rules the whole prompt is accounted: the credit is the decode positions (and the first logits) only.
        let d10 =
            palw_tir_fp_derive_work_v1(&class, p, d, p, PalwFpPrefixStateV1::genesis(Hash64::default()), 1 << 40, false).unwrap();
        assert!(d10.derived.credited_leaves < w.derived.total_leaves && d10.derived.credited_leaves > 0);
        assert!(
            d10.credited_compute < w.credited_compute && d10.credited_compute > 0,
            "the prefill is priced at zero, never negative"
        );
        // A count the class cannot hold, and a ladder below the run, are refused by name.
        assert!(
            palw_tir_fp_derive_work_v1(
                &class,
                p,
                layout.max_context + 1,
                0,
                PalwFpPrefixStateV1::genesis(Hash64::default()),
                1 << 40,
                false
            )
            .is_err()
        );
        assert!(
            palw_tir_fp_derive_work_v1(
                &class,
                p,
                d,
                0,
                PalwFpPrefixStateV1::genesis(Hash64::default()),
                w.derived.total_leaves - 1,
                false
            )
            .is_err()
        );
        // A consumed prefix state is out of v1's scope.
        let state = PalwFpPrefixStateV1 {
            class_id: x.binding.class.class_id(&x.binding.artifact_root),
            prefix_tokens: 1,
            state_root: Hash64::from_u64_word(9),
        };
        assert!(palw_tir_fp_derive_work_v1(&class, p, d, 0, state, 1 << 40, false).is_err());
    }

    /// The class is rebuilt only from a layout the record's digest names.
    #[test]
    fn tir_fp_class_refuses_a_layout_the_record_does_not_bind() {
        let (record, layout, _) = tiny();
        let mut other = layout.clone();
        other.checkpoint_interval += 1;
        assert!(palw_tir_fp_class_v1(&record, &layout).is_ok());
        assert!(palw_tir_fp_class_v1(&record, &other).is_err());
    }

    /// Coverage by ONE family's kernel set.
    #[test]
    fn tir_fp_lane_is_covered_by_one_certified_family() {
        use crate::palw_e2e_adjudicability::{PalwE2eCoveringV1, PalwE2eFamilyV1, palw_e2e_family_id_v1};
        let (record, _, _) = tiny();
        let program = misaka_palw_tir::TirProgramV1::decode_canonical(&record.program).unwrap();
        let reachable = crate::palw_tir_admission_v1::palw_tir_reachable_prims_v1(&program);
        let family = |ids: std::collections::BTreeSet<Hash64>| PalwE2eFamilyV1 {
            family_id: palw_e2e_family_id_v1("TEST-TIR-FP"),
            drilled_class_id: Hash64::from_u64_word(1),
            kernel_ids: ids,
            covering: PalwE2eCoveringV1::default(),
        };
        let whole = family(reachable.clone());
        assert!(palw_tir_fp_lane_covered_v1(&record, [&whole]).is_ok());
        let mut split: Vec<_> = reachable.iter().copied().collect();
        let tail = split.split_off(split.len() / 2);
        let (a, b) = (family(split.into_iter().collect()), family(tail.into_iter().collect()));
        assert!(palw_tir_fp_lane_covered_v1(&record, [&a, &b]).is_err(), "two halves are not one certificate");
        assert!(palw_tir_fp_lane_covered_v1(&record, std::iter::empty()).is_err());
    }
}
