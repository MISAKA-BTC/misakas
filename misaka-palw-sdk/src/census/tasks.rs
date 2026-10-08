//! **The declared task and the canonical job profile it needs** (RFC-0002 §II.10.1: "the whole declared task must have a canonical
//! job, inputs, output, court path and artifact"). A table, versioned by [`TASK_PROFILES_VERSION`]: each Hub `pipeline_tag` names the
//! profile that would carry it in this build, or none. Nothing here reads a model name.
//!
//! The profiles this build carries: the text decoder class (RFC-0002, `palw_tir_v1`), and RFC-0003's pipeline classes under
//! `palw_gen_v1` — the embedding profile (text or image input), the image profile (text-to-image) and the text pipeline (an
//! encoder–decoder; a vision-language model's two stages). `PalwGenProfileV1::Audio` and `::Video` are tags without a job body, so an
//! audio or video task has no profile. Classification, detection, segmentation and masked-language heads have no canonical job.

use serde::Serialize;

pub const TASK_PROFILES_VERSION: &str = "misaka.palw.hf-census-tasks.v1";

/// The rules `census::listing::task_of` infers a missing task by: v2 (2026-10-04) adds transformers head classes and GGUF
/// architecture names to v1's causal-LM classes and PEFT task types; v3 (2026-10-08) reads every PEFT `task_type` and, for an adapter
/// that declares none, the task of its pinned base's head class (`store::base_task_of`, applied where the base was read).
pub const TASK_INFERENCE_VERSION: &str = "inference-v3";

/// What carries a task.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Profile {
    /// RFC-0002's text decoder class: token ids in, next-token logits out (`palw_tir_v1`).
    TextDecoder,
    /// RFC-0003's text pipeline: an encoder–decoder (`palw_gen_v1`).
    GenText,
    /// RFC-0003's embedding profile (`palw_gen_v1`).
    GenEmbedding,
    /// RFC-0003's image profile, text-to-image (`palw_gen_v1`).
    GenImage,
    /// The task needs a part (a vision or audio input stage) the class this build's preflight produces does not compute: the text
    /// stage is a separately scoped class (§II.10.1), the repository's task is not credited.
    PartialTextStage,
    /// No canonical job profile.
    None,
}

impl Profile {
    /// The fence the profile's classes need, by its params name.
    pub fn fence(self) -> Option<&'static str> {
        match self {
            Profile::TextDecoder => Some("palw_tir_v1"),
            Profile::GenText | Profile::GenEmbedding | Profile::GenImage => Some("palw_gen_v1"),
            Profile::PartialTextStage | Profile::None => None,
        }
    }

    /// The class is an RFC-0003 pipeline class (registered and admitted by the generative lane's rules).
    pub fn is_pipeline(self) -> bool {
        matches!(self, Profile::GenText | Profile::GenEmbedding | Profile::GenImage)
    }
}

/// A task's row: its group (a stratum) and its profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct TaskRow {
    pub task: &'static str,
    pub group: &'static str,
    pub profile: Profile,
}

const fn row(task: &'static str, group: &'static str, profile: Profile) -> TaskRow {
    TaskRow { task, group, profile }
}

/// The table. A tag that is not here is the group `other` with no profile.
pub const TASKS_V1: &[TaskRow] = &[
    row("text-generation", "text-generation", Profile::TextDecoder),
    row("text2text-generation", "text2text", Profile::GenText),
    row("translation", "text2text", Profile::GenText),
    row("summarization", "text2text", Profile::GenText),
    row("feature-extraction", "embedding", Profile::GenEmbedding),
    row("sentence-similarity", "embedding", Profile::GenEmbedding),
    row("image-feature-extraction", "embedding", Profile::GenEmbedding),
    row("text-to-image", "image-generation", Profile::GenImage),
    row("image-text-to-text", "multimodal-text", Profile::PartialTextStage),
    row("visual-question-answering", "multimodal-text", Profile::PartialTextStage),
    row("document-question-answering", "multimodal-text", Profile::PartialTextStage),
    row("video-text-to-text", "multimodal-text", Profile::PartialTextStage),
    row("audio-text-to-text", "multimodal-text", Profile::PartialTextStage),
    row("any-to-any", "multimodal-text", Profile::PartialTextStage),
    row("image-to-text", "multimodal-text", Profile::None),
    row("fill-mask", "nlu", Profile::None),
    row("text-classification", "nlu", Profile::None),
    row("token-classification", "nlu", Profile::None),
    row("zero-shot-classification", "nlu", Profile::None),
    row("question-answering", "nlu", Profile::None),
    row("table-question-answering", "nlu", Profile::None),
    row("text-ranking", "nlu", Profile::None),
    row("multiple-choice", "nlu", Profile::None),
    row("image-to-image", "image-generation", Profile::None),
    row("unconditional-image-generation", "image-generation", Profile::None),
    row("image-to-3d", "image-generation", Profile::None),
    row("text-to-3d", "image-generation", Profile::None),
    row("image-classification", "vision", Profile::None),
    row("object-detection", "vision", Profile::None),
    row("image-segmentation", "vision", Profile::None),
    row("depth-estimation", "vision", Profile::None),
    row("zero-shot-image-classification", "vision", Profile::None),
    row("zero-shot-object-detection", "vision", Profile::None),
    row("mask-generation", "vision", Profile::None),
    row("keypoint-detection", "vision", Profile::None),
    row("video-classification", "vision", Profile::None),
    row("image-text-to-image", "vision", Profile::None),
    row("automatic-speech-recognition", "audio", Profile::None),
    row("text-to-speech", "audio", Profile::None),
    row("text-to-audio", "audio", Profile::None),
    row("audio-classification", "audio", Profile::None),
    row("audio-to-audio", "audio", Profile::None),
    row("voice-activity-detection", "audio", Profile::None),
    row("text-to-video", "video", Profile::None),
    row("image-to-video", "video", Profile::None),
    row("video-to-video", "video", Profile::None),
    row("reinforcement-learning", "rl-robotics", Profile::None),
    row("robotics", "rl-robotics", Profile::None),
    row("tabular-classification", "tabular-ts", Profile::None),
    row("tabular-regression", "tabular-ts", Profile::None),
    row("time-series-forecasting", "tabular-ts", Profile::None),
    row("graph-ml", "other", Profile::None),
    row("other", "other", Profile::None),
];

/// The row of a task; an unknown tag is `other` with no profile.
pub fn task_row(task: &str) -> TaskRow {
    TASKS_V1.iter().find(|r| r.task == task).copied().unwrap_or(TaskRow { task: "other", group: "other", profile: Profile::None })
}

/// The table's identity (BLAKE2b-256 of its rows), printed in every report so a verdict is attributable to the table it used.
pub fn tasks_digest() -> String {
    let mut st = blake2b_simd::Params::new().hash_length(32).key(b"misaka-palw/hf-census-tasks/v1").to_state();
    for r in TASKS_V1 {
        st.update(r.task.as_bytes());
        st.update(&[0]);
        st.update(r.group.as_bytes());
        st.update(&[0]);
        st.update(format!("{:?}", r.profile).as_bytes());
        st.update(b"\n");
    }
    // The inference rules for a repository with no declared task are part of the table's identity.
    st.update(TASK_INFERENCE_VERSION.as_bytes());
    crate::preflight::source::hex(st.finalize().as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_task_appears_once_and_an_unknown_tag_has_no_profile() {
        let mut seen = std::collections::BTreeSet::new();
        for r in TASKS_V1 {
            assert!(seen.insert(r.task), "{} twice", r.task);
        }
        assert_eq!(task_row("a-new-hub-task").profile, Profile::None);
        assert_eq!(task_row("text-generation").profile.fence(), Some("palw_tir_v1"));
        assert_eq!(task_row("image-text-to-text").profile, Profile::PartialTextStage);
        assert_eq!(tasks_digest().len(), 64);
    }
}
