//! **The chat template a lane renders — chosen once, spelled once.**
//!
//! [`crate::tokenizer::qwen_chat_prompt`] renders *Qwen2.5's* template, and its own doc says what
//! this module exists to satisfy: *"A model whose template differs needs its own renderer, and
//! that is a per-class fact like every other."* The QWEN36 lane's model is such a model, and
//! never got one.
//!
//! # The defect this closes
//!
//! `Qwen3.5-2B-Q4_K_M.gguf`'s own `tokenizer.chat_template` ends its generation prompt with:
//!
//! ```text
//! {%- if add_generation_prompt %}
//!     {{- '<|im_start|>assistant\n' }}
//!     {%- if enable_thinking is defined and enable_thinking is true %}
//!         {{- '<think>\n' }}
//!     {%- else %}
//!         {{- '<think>\n\n</think>\n\n' }}
//!     {%- endif %}
//! {%- endif %}
//! ```
//!
//! **There is no branch that stops at `assistant\n`.** Under the model's own template the think
//! block is part of the PROMPT in both modes and the model never generates it. This tree stopped
//! at `assistant\n`, so a reasoning model emitted the block itself, into the ANSWER bytes, where
//! `grammar.canonicalize` sees leading non-JSON and refuses at column 1. Measured on the shipped
//! assembly: 4 of 8 gate cases produced the right DSL and every one was refused; 2 more spent the
//! whole decode budget on an open reasoning trace and never reached the DSL.
//!
//! # Which mode, and why it is the closed one
//!
//! [`QwenChatVariantV1::ChatmlThinkClosed`] places `<think>\n\n</think>\n\n` — the model's own
//! `enable_thinking is not true` branch, i.e. its DEFAULT. The other branch (`<think>\n`) leaves
//! the block OPEN, which asks the model to emit a reasoning trace and a closing `</think>` before
//! the first byte of DSL. This lane cannot pay for that: the class's context is the whole budget
//! (prompt + answer), the answers that pass are tens of tokens of JSON, and the gate measured two
//! cases burning their ENTIRE decode budget inside an unclosed trace. The closed block costs four
//! prompt tokens once — `<think>`, `\n\n`, `</think>`, `\n\n` — and buys the guarantee that the
//! first token the model generates is the first token of the answer. Four tokens of prefill
//! against an unbounded trace is not a close call.
//!
//! # Which lane gets it
//!
//! Read off the model's own metadata, never off a name: a tokenizer that declares `<think>` and
//! `</think>` as added tokens is a model whose template places them, and a model that does not
//! declare them cannot be given them at all — a `Special(id)` the tokenizer does not hold is
//! refused by [`crate::fp_worker::prompt_ids_for_input_v1`]. This is the same rule one level up,
//! where `<|im_start|>`/`<|im_end|>` decide ChatML against plain markers, so no caller is ever
//! guessing. Measured on the two shipped tiers: the dense tier's Qwen2.5-1.5B `tokenizer.json`
//! declares 22 added tokens and NEITHER think marker, so the dense lane selects
//! [`QwenChatVariantV1::Chatml`] and its prompt does not move by one byte; the QWEN36 lane's
//! `Qwen3.5-2B-Q4_K_M.gguf` declares both (248068 / 248069, `token_type` 4). Both facts are
//! pinned by tests in this file.
//!
//! # Why the preamble is `Special(id)` and not text
//!
//! ADR-0079 Decision 7: a `Text` segment is encoded with `encode_without_specials`, so a
//! stranger's `<|im_start|>` stays ordinary text — and so does a template's `<think>`. Spelling
//! the preamble as text produced the BPE pieces `[13314, 741, 29, 271, 510, 26003, 29, 271]`
//! instead of `[248068, 271, 248069, 271]`: a MISSPELLED think block, which the model answered
//! with degenerate closing tags. The segment API's `Special(id)` is the documented way a template
//! names a control token, and this module never accepts an id it did not look up by NAME.
//!
//! # Why a new template id
//!
//! A template id names an exact transform from messages to model input. `chat-segments/v1` names
//! the transform that ends at `assistant\n`; the think-closed transform ends four tokens later and
//! therefore hashes to a different `prompt_token_ids_hash_v2`, which is part of job identity.
//! Redefining `chat-segments/v1` in place would let an old prompt and a new prompt carry the same
//! id while differing — the exact failure the file next door already paid for once, where
//! `plain-markers-segments/v1` had to be minted because `plain-markers/v1` had produced *"same
//! rendered string, different ids, and ids are what consensus sees"*. So the think-closed
//! transform gets [`TEMPLATE_ID_CHAT_SEGMENTS_THINK_CLOSED_V1`], beside the old id and never on
//! top of it.
//!
//! # One spelling
//!
//! Every caller — the gateway's `build_prompt`, the model gates — builds its segments HERE.
//! [`qwen_chat_prompt_plan_v1`] additionally re-renders the plan it just built, from the segments
//! alone, and compares that against [`crate::tokenizer::qwen_chat_prompt`] plus the variant's
//! preamble. Two independent constructions of the same string, checked on every call in release
//! as well as debug: a divergence between this tree's two spellings of the template is an `Err`
//! that refuses the prompt, not a silently different prompt.

use crate::tokenizer::qwen_chat_prompt;
use kaspa_consensus_core::palw_freeprompt_v3::PalwFpPromptSegmentV1;

/// ADR-0077 Decision 6: the ChatML transform that ends the generation prompt at `assistant\n`.
/// Correct for Qwen2.5 and for every non-reasoning ChatML model.
pub const TEMPLATE_ID_CHAT_SEGMENTS_V1: &str = "misaka-palw/fp-gateway-template/chat-segments/v1";
/// The same transform with the model's own closed think block appended to the generation prompt.
/// A distinct id because the ids it produces differ, and ids are what consensus sees.
pub const TEMPLATE_ID_CHAT_SEGMENTS_THINK_CLOSED_V1: &str = "misaka-palw/fp-gateway-template/chat-segments-think-closed/v1";

/// The ChatML markers, by NAME. Nothing in this module ever holds an id it did not look up.
pub const CHATML_START: &str = "<|im_start|>";
pub const CHATML_END: &str = "<|im_end|>";
/// The reasoning markers, by NAME.
pub const THINK_OPEN: &str = "<think>";
pub const THINK_CLOSE: &str = "</think>";

/// The generation-prompt tail of the model's own `enable_thinking is not true` branch, as TEXT.
/// The bytes are what it MEANS; [`qwen_chat_prompt_plan_v1`] carries the two markers as
/// `Special(id)` and only the newlines as `Text`.
pub const CLOSED_THINK_PREAMBLE: &str = "<think>\n\n</think>\n\n";

/// What went wrong. Every variant names the fact it checked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QwenChatTemplateError {
    /// The plan's segments do not re-render to this tree's other spelling of the same template.
    /// Only reachable by editing one spelling and not the other, which is the point.
    Drift { from_segments: String, from_renderer: String },
    /// The variant asked for needs a marker this model does not declare.
    MissingMarker(&'static str),
}

impl std::fmt::Display for QwenChatTemplateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Drift { from_segments, from_renderer } => write!(
                f,
                "the chat template's two spellings disagree: the segments render {from_segments:?} and \
                 tokenizer::qwen_chat_prompt renders {from_renderer:?}"
            ),
            Self::MissingMarker(name) => write!(f, "this model's tokenizer does not declare {name}"),
        }
    }
}

impl std::error::Error for QwenChatTemplateError {}

/// **Which ChatML transform this model's own metadata selects.**
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QwenChatVariantV1 {
    /// Ends the generation prompt at `assistant\n`.
    Chatml,
    /// Ends it with the model's closed think block — the reasoning lane.
    ChatmlThinkClosed,
}

impl QwenChatVariantV1 {
    /// The transform's id. A change of transform is a change of id.
    pub const fn template_id(self) -> &'static str {
        match self {
            Self::Chatml => TEMPLATE_ID_CHAT_SEGMENTS_V1,
            Self::ChatmlThinkClosed => TEMPLATE_ID_CHAT_SEGMENTS_THINK_CLOSED_V1,
        }
    }

    /// What this transform appends after `assistant\n`, as text.
    pub const fn generation_preamble(self) -> &'static str {
        match self {
            Self::Chatml => "",
            Self::ChatmlThinkClosed => CLOSED_THINK_PREAMBLE,
        }
    }

    /// The marker names this transform needs the model to declare — its full requirement, so a
    /// caller reporting "why this variant" can name the evidence rather than assert a conclusion.
    pub const fn required_markers(self) -> &'static [&'static str] {
        match self {
            Self::Chatml => &[CHATML_START, CHATML_END],
            Self::ChatmlThinkClosed => &[CHATML_START, CHATML_END, THINK_OPEN, THINK_CLOSE],
        }
    }
}

/// Look up a control token by NAME in a manifest's `special_tokens` table.
pub fn special_id(specials: &[(String, u32)], name: &str) -> Option<u32> {
    specials.iter().find(|(n, _)| n == name).map(|(_, id)| *id)
}

/// **The variant this model's declared tokens select**, or `None` when the model is not a ChatML
/// model at all (the caller then falls back to plain markers).
///
/// A model that declares both think markers is a model whose template places them; one that does
/// not could not be given them anyway.
pub fn qwen_chat_variant_v1(specials: &[(String, u32)]) -> Option<QwenChatVariantV1> {
    if special_id(specials, CHATML_START).is_none() || special_id(specials, CHATML_END).is_none() {
        return None;
    }
    if special_id(specials, THINK_OPEN).is_some() && special_id(specials, THINK_CLOSE).is_some() {
        return Some(QwenChatVariantV1::ChatmlThinkClosed);
    }
    Some(QwenChatVariantV1::Chatml)
}

/// **The prompt, as segments** — what a caller sends and what it would display.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QwenChatPlanV1 {
    pub variant: QwenChatVariantV1,
    pub template_id: &'static str,
    pub segments: Vec<PalwFpPromptSegmentV1>,
    /// The control-token ids this plan placed, in the order it placed them.
    pub declared_specials: Vec<u32>,
    /// What a person reading this prompt would see.
    pub displayed: String,
}

/// **Build the prompt for the variant this model selects.** `Ok(None)` means "not a ChatML model".
///
/// `turns` is `(role, content)` in order; a `system` turn is just a turn, exactly as
/// [`qwen_chat_prompt`] renders it. Role vocabulary is the caller's business — this is a
/// transform, not an entrance.
pub fn qwen_chat_prompt_plan_v1(
    specials: &[(String, u32)],
    turns: &[(&str, &str)],
) -> Result<Option<QwenChatPlanV1>, QwenChatTemplateError> {
    let Some(variant) = qwen_chat_variant_v1(specials) else { return Ok(None) };
    qwen_chat_prompt_plan_for_variant_v1(variant, specials, turns).map(Some)
}

/// The same builder with the variant FORCED — for a harness that has to reproduce the other
/// transform on the same model (the red arm of the QWEN36 gate is exactly this). Production
/// callers use [`qwen_chat_prompt_plan_v1`], which lets the model choose.
pub fn qwen_chat_prompt_plan_for_variant_v1(
    variant: QwenChatVariantV1,
    specials: &[(String, u32)],
    turns: &[(&str, &str)],
) -> Result<QwenChatPlanV1, QwenChatTemplateError> {
    let mut ids = Vec::new();
    for name in variant.required_markers() {
        ids.push((*name, special_id(specials, name).ok_or(QwenChatTemplateError::MissingMarker(name))?));
    }
    let id_of = |name: &str| ids.iter().find(|(n, _)| *n == name).map(|(_, id)| *id).expect("required_markers was just resolved");
    let start = id_of(CHATML_START);
    let end = id_of(CHATML_END);

    let mut segments: Vec<PalwFpPromptSegmentV1> = Vec::new();
    let mut declared_specials: Vec<u32> = Vec::new();
    let special = |id: u32, segments: &mut Vec<PalwFpPromptSegmentV1>, declared: &mut Vec<u32>| {
        segments.push(PalwFpPromptSegmentV1::Special(id));
        declared.push(id);
    };
    let text = |s: &str, segments: &mut Vec<PalwFpPromptSegmentV1>| {
        if !s.is_empty() {
            segments.push(PalwFpPromptSegmentV1::Text(s.as_bytes().to_vec()));
        }
    };

    for (role, content) in turns {
        special(start, &mut segments, &mut declared_specials);
        // The role name and the newline ride with the user's text in ONE text segment: they are
        // ordinary tokens in this template, and splitting them would only add segments a reader
        // has to reassemble. The control tokens are the only `Special`s here, which is exactly
        // what the gateway's SA-3 subsequence check then asserts about the committed ids.
        text(&format!("{role}\n{content}"), &mut segments);
        special(end, &mut segments, &mut declared_specials);
        text("\n", &mut segments);
    }
    special(start, &mut segments, &mut declared_specials);
    text("assistant\n", &mut segments);

    if variant == QwenChatVariantV1::ChatmlThinkClosed {
        // The two markers are ids and the newlines between them are text: `<think>` spelled as
        // text is a different, wrong token sequence (see the module doc).
        special(id_of(THINK_OPEN), &mut segments, &mut declared_specials);
        text("\n\n", &mut segments);
        special(id_of(THINK_CLOSE), &mut segments, &mut declared_specials);
        text("\n\n", &mut segments);
    }

    // The anti-drift check, always on. `displayed` is re-derived from the SEGMENTS — the thing
    // that will actually be tokenized — and compared against this tree's other spelling of the
    // same template. Two independent constructions; a divergence refuses the prompt.
    let names: Vec<(u32, &str)> = ids.iter().map(|(n, id)| (*id, *n)).collect();
    let displayed = render_segments_v1(&segments, &names);
    let expected = format!("{}{}", qwen_chat_prompt(None, turns), variant.generation_preamble());
    if displayed != expected {
        return Err(QwenChatTemplateError::Drift { from_segments: displayed, from_renderer: expected });
    }

    Ok(QwenChatPlanV1 { variant, template_id: variant.template_id(), segments, declared_specials, displayed })
}

/// Render a segment list as the string a reader would see: a `Special` shows the NAME it was
/// looked up by, a `Text` shows its bytes. An id with no name in `names` renders as `<|id:N|>`,
/// which cannot silently pass for a marker.
fn render_segments_v1(segments: &[PalwFpPromptSegmentV1], names: &[(u32, &str)]) -> String {
    let mut out = String::new();
    for segment in segments {
        match segment {
            PalwFpPromptSegmentV1::Special(id) => match names.iter().find(|(candidate, _)| candidate == id) {
                Some((_, name)) => out.push_str(name),
                None => out.push_str(&format!("<|id:{id}|>")),
            },
            PalwFpPromptSegmentV1::Text(bytes) => out.push_str(&String::from_utf8_lossy(bytes)),
        }
    }
    out
}

/// **Qwen3.5's chat template, rendered as text** — the sibling of
/// [`qwen_chat_prompt`] that the QWEN36 lane needed and did not have.
///
/// The plain-text spelling, for a caller that wants the string rather than the segments. The
/// segment builder above is the one production uses, and it checks itself against this.
pub fn qwen35_chat_prompt(system: Option<&str>, turns: &[(&str, &str)]) -> String {
    format!("{}{}", qwen_chat_prompt(system, turns), CLOSED_THINK_PREAMBLE)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The dense tier's added-token table, MEASURED from the shipped
    /// `models/qwen2.5-1.5b/tokenizer.json` — all 22 entries, ids included, in file order.
    /// Reproduced here so the dense lane's invariance is a fact this test suite holds rather than
    /// a fact about a file on one laptop.
    fn dense_qwen25_specials() -> Vec<(String, u32)> {
        [
            ("<|endoftext|>", 151_643u32),
            ("<|im_start|>", 151_644),
            ("<|im_end|>", 151_645),
            ("<|object_ref_start|>", 151_646),
            ("<|object_ref_end|>", 151_647),
            ("<|box_start|>", 151_648),
            ("<|box_end|>", 151_649),
            ("<|quad_start|>", 151_650),
            ("<|quad_end|>", 151_651),
            ("<|vision_start|>", 151_652),
            ("<|vision_end|>", 151_653),
            ("<|vision_pad|>", 151_654),
            ("<|image_pad|>", 151_655),
            ("<|video_pad|>", 151_656),
            ("<tool_call>", 151_657),
            ("</tool_call>", 151_658),
            ("<|fim_prefix|>", 151_659),
            ("<|fim_middle|>", 151_660),
            ("<|fim_suffix|>", 151_661),
            ("<|fim_pad|>", 151_662),
            ("<|repo_name|>", 151_663),
            ("<|file_sep|>", 151_664),
        ]
        .into_iter()
        .map(|(n, id)| (n.to_string(), id))
        .collect()
    }

    /// The QWEN36 lane's table, MEASURED from `Qwen3.5-2B-Q4_K_M.gguf`'s
    /// `tokenizer.ggml.tokens` / `token_type`: the two ChatML markers are `token_type` 3
    /// (CONTROL) and the two think markers are `token_type` 4 (USER_DEFINED) — both land in the
    /// added-token table, which is why `Special(id)` accepts them.
    fn qwen35_specials() -> Vec<(String, u32)> {
        [
            ("<|endoftext|>", 248_044u32),
            ("<|im_start|>", 248_045),
            ("<|im_end|>", 248_046),
            ("<think>", 248_068),
            ("</think>", 248_069),
        ]
        .into_iter()
        .map(|(n, id)| (n.to_string(), id))
        .collect()
    }

    /// **Constraint: the dense/A16 lane does not move, byte for byte.**
    ///
    /// Not "the variant is Chatml" — the whole plan, segment by segment, against the transform
    /// that was already proven with real SMF and STL through the shipped transformers. This is the
    /// literal segment list `palw-model-gate`'s reproduction builds, so if this passes, the dense
    /// gate's prompt ids cannot have moved: ids are a function of (segments, tokenizer) and
    /// neither changed.
    #[test]
    fn the_dense_lane_selects_the_old_transform_and_its_segments_are_unchanged() {
        let specials = dense_qwen25_specials();
        assert_eq!(
            qwen_chat_variant_v1(&specials),
            Some(QwenChatVariantV1::Chatml),
            "the Qwen2.5 table declares 22 added tokens and neither think marker; it must not select the reasoning transform"
        );
        assert!(special_id(&specials, THINK_OPEN).is_none(), "checked: <think> is absent from the dense table");
        assert!(special_id(&specials, THINK_CLOSE).is_none(), "checked: </think> is absent from the dense table");

        let plan = qwen_chat_prompt_plan_v1(&specials, &[("user", "Emit the minimal cad/v1 DSL for a box. Output only JSON.")])
            .expect("the dense plan builds")
            .expect("the dense table is ChatML");
        assert_eq!(plan.template_id, TEMPLATE_ID_CHAT_SEGMENTS_V1);
        assert_eq!(plan.declared_specials, vec![151_644, 151_645, 151_644]);
        assert_eq!(
            plan.segments,
            vec![
                PalwFpPromptSegmentV1::Special(151_644),
                PalwFpPromptSegmentV1::Text(b"user\nEmit the minimal cad/v1 DSL for a box. Output only JSON.".to_vec()),
                PalwFpPromptSegmentV1::Special(151_645),
                PalwFpPromptSegmentV1::Text(b"\n".to_vec()),
                PalwFpPromptSegmentV1::Special(151_644),
                PalwFpPromptSegmentV1::Text(b"assistant\n".to_vec()),
            ],
            "the dense lane's segments are the ones the dense tier's evidence was measured against"
        );
        assert_eq!(
            plan.displayed,
            "<|im_start|>user\nEmit the minimal cad/v1 DSL for a box. Output only JSON.<|im_end|>\n<|im_start|>assistant\n"
        );
        assert!(!plan.displayed.contains("think"), "not one byte of the reasoning transform reaches the dense lane");
        // And the multi-turn shape, including a system turn, is the old one too.
        let multi = qwen_chat_prompt_plan_v1(&specials, &[("system", "s"), ("user", "u"), ("assistant", "a"), ("user", "u2")])
            .expect("builds")
            .expect("ChatML");
        assert_eq!(
            multi.displayed,
            "<|im_start|>system\ns<|im_end|>\n<|im_start|>user\nu<|im_end|>\n<|im_start|>assistant\na<|im_end|>\n\
             <|im_start|>user\nu2<|im_end|>\n<|im_start|>assistant\n"
        );
        assert_eq!(
            multi.displayed,
            crate::tokenizer::qwen_chat_prompt(None, &[("system", "s"), ("user", "u"), ("assistant", "a"), ("user", "u2")])
        );
    }

    /// **The QWEN36 lane gets the model's own generation prompt**, and the two markers are ids.
    #[test]
    fn the_reasoning_lane_places_a_closed_think_block_as_ids() {
        let specials = qwen35_specials();
        assert_eq!(qwen_chat_variant_v1(&specials), Some(QwenChatVariantV1::ChatmlThinkClosed));

        let plan = qwen_chat_prompt_plan_v1(&specials, &[("user", "hi")]).expect("builds").expect("ChatML");
        assert_eq!(plan.template_id, TEMPLATE_ID_CHAT_SEGMENTS_THINK_CLOSED_V1);
        assert_ne!(plan.template_id, TEMPLATE_ID_CHAT_SEGMENTS_V1, "an old and a new prompt must never carry the same id");
        assert_eq!(
            plan.segments,
            vec![
                PalwFpPromptSegmentV1::Special(248_045),
                PalwFpPromptSegmentV1::Text(b"user\nhi".to_vec()),
                PalwFpPromptSegmentV1::Special(248_046),
                PalwFpPromptSegmentV1::Text(b"\n".to_vec()),
                PalwFpPromptSegmentV1::Special(248_045),
                PalwFpPromptSegmentV1::Text(b"assistant\n".to_vec()),
                PalwFpPromptSegmentV1::Special(248_068),
                PalwFpPromptSegmentV1::Text(b"\n\n".to_vec()),
                PalwFpPromptSegmentV1::Special(248_069),
                PalwFpPromptSegmentV1::Text(b"\n\n".to_vec()),
            ]
        );
        assert_eq!(plan.declared_specials, vec![248_045, 248_046, 248_045, 248_068, 248_069]);
        assert_eq!(plan.displayed, "<|im_start|>user\nhi<|im_end|>\n<|im_start|>assistant\n<think>\n\n</think>\n\n");
        assert_eq!(plan.displayed, qwen35_chat_prompt(None, &[("user", "hi")]));

        // Not one think marker rides inside a Text segment — that is the misspelling that made the
        // model answer in degenerate closing tags.
        for segment in &plan.segments {
            if let PalwFpPromptSegmentV1::Text(bytes) = segment {
                let text = String::from_utf8(bytes.clone()).expect("the template's text is UTF-8");
                assert!(!text.contains("think"), "a think marker rode inside a text segment: {text:?}");
                assert!(!text.contains("<|im_"), "a ChatML marker rode inside a text segment: {text:?}");
            }
        }
    }

    /// **The forced arm reproduces the OLD transform on the NEW model** — the gate's red arm, and
    /// the proof that the two transforms differ where they are supposed to and nowhere else.
    #[test]
    fn the_old_transform_can_still_be_forced_and_differs_only_in_its_tail() {
        let specials = qwen35_specials();
        let old = qwen_chat_prompt_plan_for_variant_v1(QwenChatVariantV1::Chatml, &specials, &[("user", "hi")]).expect("builds");
        let new =
            qwen_chat_prompt_plan_for_variant_v1(QwenChatVariantV1::ChatmlThinkClosed, &specials, &[("user", "hi")]).expect("builds");
        assert_eq!(old.displayed, "<|im_start|>user\nhi<|im_end|>\n<|im_start|>assistant\n");
        assert_eq!(new.displayed, format!("{}{}", old.displayed, CLOSED_THINK_PREAMBLE));
        assert_eq!(new.segments[..old.segments.len()], old.segments[..], "the new transform is the old one plus a tail");
        assert_eq!(new.segments.len(), old.segments.len() + 4);
        assert_eq!(old.template_id, TEMPLATE_ID_CHAT_SEGMENTS_V1);

        // A model without the markers cannot be forced into the reasoning transform — the error
        // names the marker it wanted.
        let err =
            qwen_chat_prompt_plan_for_variant_v1(QwenChatVariantV1::ChatmlThinkClosed, &dense_qwen25_specials(), &[("user", "hi")])
                .unwrap_err();
        assert_eq!(err, QwenChatTemplateError::MissingMarker(THINK_OPEN));
    }

    /// A model this gateway cannot name at all gets no plan — the caller falls back to plain
    /// markers, which is the transform it already had.
    #[test]
    fn a_model_without_chatml_markers_gets_no_chatml_plan() {
        assert_eq!(qwen_chat_variant_v1(&[]), None);
        assert_eq!(qwen_chat_prompt_plan_v1(&[], &[("user", "hi")]), Ok(None));
        // Half a ChatML model is not a ChatML model.
        let half = vec![("<|im_start|>".to_string(), 1u32)];
        assert_eq!(qwen_chat_variant_v1(&half), None);
        // …and neither is one that declares only half a think block: it gets the OLD transform,
        // never a half-placed one.
        let half_think = vec![("<|im_start|>".to_string(), 1u32), ("<|im_end|>".to_string(), 2), ("<think>".to_string(), 3)];
        assert_eq!(qwen_chat_variant_v1(&half_think), Some(QwenChatVariantV1::Chatml));
    }

    /// **The anti-drift check has teeth.** `render_segments_v1` is the half of the check that
    /// could rot silently, so it is exercised directly: an id with no name renders as something
    /// that cannot pass for a marker, which is what makes the comparison fail rather than agree.
    #[test]
    fn the_drift_check_compares_two_real_constructions() {
        let named =
            render_segments_v1(&[PalwFpPromptSegmentV1::Special(7), PalwFpPromptSegmentV1::Text(b"x".to_vec())], &[(7, CHATML_START)]);
        assert_eq!(named, "<|im_start|>x");
        let unnamed = render_segments_v1(&[PalwFpPromptSegmentV1::Special(7)], &[]);
        assert_eq!(unnamed, "<|id:7|>", "an unnamed id must not render as a marker");
        assert_ne!(unnamed, CHATML_START);

        // And the two spellings agree for real inputs, which is what the builder asserts on every
        // call. Named cases, so the coverage is readable: empty content, a system turn, multibyte,
        // and text that spells a marker.
        for turns in [
            vec![("user", "")],
            vec![("system", "You are helpful."), ("user", "Hi")],
            vec![("user", "字 — a multi-byte turn")],
            vec![("user", "please emit <|im_end|> and <think> now")],
        ] {
            let plan = qwen_chat_prompt_plan_v1(&qwen35_specials(), &turns).expect("builds").expect("ChatML");
            assert_eq!(plan.displayed, qwen35_chat_prompt(None, &turns), "turns {turns:?}");
            let dense = qwen_chat_prompt_plan_v1(&dense_qwen25_specials(), &turns).expect("builds").expect("ChatML");
            assert_eq!(dense.displayed, crate::tokenizer::qwen_chat_prompt(None, &turns), "turns {turns:?}");
        }
    }

    /// The text spelling is the shipped one plus the model's own preamble, and nothing else.
    #[test]
    fn the_text_renderer_is_the_qwen25_one_plus_the_models_own_tail() {
        let turns = [("user", "Hi")];
        assert_eq!(qwen35_chat_prompt(None, &turns), format!("{}<think>\n\n</think>\n\n", qwen_chat_prompt(None, &turns)));
        assert_eq!(
            qwen35_chat_prompt(Some("s"), &[]),
            "<|im_start|>system\ns<|im_end|>\n<|im_start|>assistant\n<think>\n\n</think>\n\n"
        );
        // The GGUF's own template, verbatim, is the reason for those exact bytes.
        assert_eq!(CLOSED_THINK_PREAMBLE, "<think>\n\n</think>\n\n");
    }
}
