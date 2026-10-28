//! **Genesis 8 — the next network's genesis model list, as an RFC-0019 edition-0 template** (user decisions 2026-10-15; dormant).
//!
//! The genesis set is eight models; ids keep the user's original 1..10 numbering (7 and 9 were dropped). Every one is an uncensored
//! variant (huihui abliterated, Dolphin where no huihui variant exists). #8 is rebuilt at 32K (user decision 2026-10-16) from huihui's
//! abliterated Qwen3.6-35B-A3B GGUF (Q8_0) with the legacy PALWQ361 converter into a new repository; the earlier
//! `Misakachain/Qwen3.6-35B-A3B-PALW-runtime` (512 positions) stays as history.
//!
//! **Scope: text generation only** (user policy 2026-10-15) — token ids in, next-token logits out. Vision towers and projectors
//! (mmproj), image/audio/video embedders and multi-token-prediction heads are not computed, not covered, not admitted as claims and
//! not rewarded: [`palw_genesis8_admits_input_v1`] refuses every input but token ids by name, and the artifact refuses such
//! components by name (`misaka_palw_tir_artifact::palwart_v3::check_text_scope_v3`). Each ships as ONE `PALWART3` artifact, and the catalog's `artifact_root` is that
//! artifact's root (`misaka_palw_tir_artifact::palwart_v3::palwart_root_v3`), which commits the legacy identity of its weight section
//! (the A16 `artifact_digest`, the TIR inventory root, or the PALWQ361 file digest) as an inner root.
//!
//! * **Dormant.** No preset reads this module. testnet-12, its preset and its DAA 14,000 release scope are untouched; the list is
//!   for the NEXT network's genesis (edition 0 is part of a network's identity, RFC-0019 §7.2).
//! * **Roots are not invented.** The class ids, artifact roots and plans are unknown until each conversion has run and the class is
//!   declared; [`palw_genesis8_edition0_v1`] builds edition 0 only from roots a caller supplies, and refuses a missing, zero, unknown
//!   or duplicated entry — there are no placeholder hashes that could be armed by mistake.
//! * **Membership is economic only** (PRINCIPLES §9): a listed class gets the 1.0 tier; every claim still needs its own Gate C
//!   (`SecurityCertified`) and court. #10 is listed as the user chose but is not end-to-end adjudicable, so its claims cannot be
//!   certified until it is: listing it grants nothing a claim has not earned.
//! * **Open integration requirement** (recorded, not implemented here): registration on the next network must bind the `PALWART3`
//!   root as the class's artifact root (today a TIR class binds its inventory root directly), with the court opening the inner
//!   inventory leaves under the inner root the container root commits.

use crate::Hash64;
use crate::palw_annual_catalog_v1::{AnnualModelCatalogV1, ApprovedModelV1, PALW_ANNUAL_CATALOG_KERNEL_VERSION_MAX_V1};

/// The weight route a genesis model is adjudicated on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwGenesisRouteV1 {
    /// BASE-0 A16 (the legacy step court).
    Base0A16,
    /// A TIR v1 program (K2-TIR kernel courts).
    TirV1,
    /// The Qwen3.6 PALWQ361 integer runtime (the legacy step court, Q36 profile).
    PalwQ361,
}

/// One genesis model: its pinned source and the container it ships in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwGenesisModelV1 {
    pub genesis_id: u8,
    /// The Hugging Face repository the `PALWART3` artifact is published to.
    pub artifact_repo: &'static str,
    pub source_repo: &'static str,
    /// The pinned source revision (a commit SHA).
    pub source_revision: &'static str,
    /// The source file a single-file source is converted from (the GGUF), empty for a sharded checkpoint.
    pub source_file: &'static str,
    pub architecture: &'static str,
    pub license: &'static str,
    pub route: PalwGenesisRouteV1,
    pub e2e_adjudicable: bool,
    /// Awaiting a user decision (#3: no official Qwen3.8 9B exists).
    pub tentative: bool,
    /// The catalog's kernel version (0: the class's own registered binding).
    pub kernel_version: u16,
    /// An uncensored variant (every genesis entry since 2026-10-16).
    pub uncensored: bool,
    /// What the checkpoint carries beyond text generation: never computed, covered, admitted or rewarded.
    pub out_of_scope: &'static [&'static str],
    /// The context the class must serve: [`PALW_GENESIS_SERVED_CONTEXT_V1`] for every genesis model (user decision 2026-10-16). A
    /// PALWQ361 artifact bakes its context into its header and rotary table, so #8 is rebuilt with `qwen36-convert --context 32768`.
    pub served_context: u32,
}

/// Every genesis model is served at a 32K context (user decision 2026-10-16).
pub const PALW_GENESIS_SERVED_CONTEXT_V1: u32 = 32_768;

/// The one scope of every genesis class.
pub const PALW_GENESIS8_SCOPE_V1: &str = "text-generation";

/// What a claim of a genesis class could be asked to read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwGenesisInputKindV1 {
    /// Token ids (the text decoder's only input).
    TokenIds,
    /// Image pixels or image rows (a vision tower, a projector, an image embedder).
    Image,
    Audio,
    Video,
    /// A multi-token-prediction draft (an MTP head).
    MtpDraft,
}

/// **The text scope, for claims**: a genesis class admits token ids and nothing else; every other input is refused by name.
pub fn palw_genesis8_admits_input_v1(kind: PalwGenesisInputKindV1) -> Result<(), &'static str> {
    match kind {
        PalwGenesisInputKindV1::TokenIds => Ok(()),
        PalwGenesisInputKindV1::Image => Err("GENESIS_SCOPE_IMAGE_REFUSED: genesis classes are text generation only"),
        PalwGenesisInputKindV1::Audio => Err("GENESIS_SCOPE_AUDIO_REFUSED: genesis classes are text generation only"),
        PalwGenesisInputKindV1::Video => Err("GENESIS_SCOPE_VIDEO_REFUSED: genesis classes are text generation only"),
        PalwGenesisInputKindV1::MtpDraft => Err("GENESIS_SCOPE_MTP_REFUSED: multi-token-prediction heads are out of scope"),
    }
}

/// **The genesis set** (registry: `scripts/genesis/genesis8_models.json`).
pub const PALW_GENESIS8_MODELS_V1: [PalwGenesisModelV1; 8] = [
    PalwGenesisModelV1 {
        genesis_id: 1,
        artifact_repo: "Misakachain/Dolphin3.0-Qwen2.5-1.5B-PALW",
        source_repo: "dphn/Dolphin3.0-Qwen2.5-1.5B",
        source_revision: "60c996e28ebee1ea3256fa9d40c06e0b20ceca67",
        source_file: "",
        architecture: "Qwen2ForCausalLM",
        license: "apache-2.0",
        route: PalwGenesisRouteV1::Base0A16,
        e2e_adjudicable: true,
        tentative: false,
        kernel_version: 0,
        uncensored: true,
        out_of_scope: &[],
        served_context: PALW_GENESIS_SERVED_CONTEXT_V1,
    },
    PalwGenesisModelV1 {
        genesis_id: 2,
        artifact_repo: "Misakachain/Huihui-Qwen3.5-2B-abliterated-PALW",
        source_repo: "huihui-ai/Huihui-Qwen3.5-2B-abliterated",
        source_revision: "b2e291a65f29a9b148981fa5299caea5d35bd4c8",
        source_file: "",
        architecture: "Qwen3_5ForConditionalGeneration",
        license: "apache-2.0",
        route: PalwGenesisRouteV1::TirV1,
        e2e_adjudicable: true,
        tentative: false,
        kernel_version: 0,
        uncensored: true,
        out_of_scope: &["vision tower model.visual.*", "MTP head mtp.*"],
        served_context: PALW_GENESIS_SERVED_CONTEXT_V1,
    },
    PalwGenesisModelV1 {
        genesis_id: 3,
        artifact_repo: "Misakachain/Huihui-Qwen3.5-9B-abliterated-PALW",
        source_repo: "huihui-ai/Huihui-Qwen3.5-9B-abliterated",
        source_revision: "05b9e7c9b978ba29bdb8f50a49c30e4b91183339",
        source_file: "",
        architecture: "Qwen3_5ForConditionalGeneration",
        license: "apache-2.0",
        route: PalwGenesisRouteV1::TirV1,
        e2e_adjudicable: true,
        tentative: true,
        kernel_version: 0,
        uncensored: true,
        out_of_scope: &["vision tower model.visual.*", "MTP head mtp.*"],
        served_context: PALW_GENESIS_SERVED_CONTEXT_V1,
    },
    PalwGenesisModelV1 {
        genesis_id: 4,
        artifact_repo: "Misakachain/Huihui-gemma-4-12B-it-abliterated-PALW",
        source_repo: "huihui-ai/Huihui-gemma-4-12B-it-abliterated",
        source_revision: "060ea173c4d1e30b636819f5281e7791b6e9be2f",
        source_file: "",
        architecture: "Gemma4UnifiedForConditionalGeneration",
        license: "apache-2.0",
        route: PalwGenesisRouteV1::TirV1,
        e2e_adjudicable: true,
        tentative: false,
        kernel_version: 0,
        uncensored: true,
        out_of_scope: &["image embedder model.vision_embedder.* / model.embed_vision.*", "audio embedder model.embed_audio.*"],
        served_context: PALW_GENESIS_SERVED_CONTEXT_V1,
    },
    PalwGenesisModelV1 {
        genesis_id: 5,
        artifact_repo: "Misakachain/Huihui-gpt-oss-20b-BF16-abliterated-PALW",
        source_repo: "huihui-ai/Huihui-gpt-oss-20b-BF16-abliterated",
        source_revision: "b73ec897fe872d5e3fea830d516520537decdde6",
        source_file: "",
        architecture: "GptOssForCausalLM",
        license: "apache-2.0",
        route: PalwGenesisRouteV1::TirV1,
        e2e_adjudicable: true,
        tentative: false,
        kernel_version: 0,
        uncensored: true,
        out_of_scope: &[],
        served_context: PALW_GENESIS_SERVED_CONTEXT_V1,
    },
    PalwGenesisModelV1 {
        genesis_id: 6,
        artifact_repo: "Misakachain/Huihui-Qwen3.8-27B-Coder390-EfficientThink-abliterated-PALW",
        source_repo: "huihui-ai/Huihui-Qwen3.8-27B-Coder390-EfficientThink-abliterated-GGUF",
        source_revision: "a0b17c71d9943529aefe88b83f494bf01c6b4d68",
        source_file: "Huihui-Qwen3.8-27B-Coder390-EfficientThink-abliterated-bf16.gguf",
        architecture: "Qwen3_5ForCausalLM (GGUF qwen35)",
        license: "apache-2.0",
        route: PalwGenesisRouteV1::TirV1,
        e2e_adjudicable: true,
        tentative: false,
        kernel_version: 0,
        uncensored: true,
        out_of_scope: &["vision projector mmproj-model-bf16.gguf (not fetched)", "MTP block blk.64.nextn.*"],
        served_context: PALW_GENESIS_SERVED_CONTEXT_V1,
    },
    PalwGenesisModelV1 {
        genesis_id: 8,
        artifact_repo: "Misakachain/Huihui-Qwen3.6-35B-A3B-abliterated-PALW",
        source_repo: "huihui-ai/Huihui-Qwen3.6-35B-A3B-abliterated-MTP-GGUF",
        source_revision: "acd5abe0c5ebd064f3863ca7d4bba2d924ec6544",
        source_file: "Huihui-Qwen3.6-35B-A3B-abliterated-ggml-model-Q8_0.gguf",
        architecture: "Qwen3_5MoeForConditionalGeneration",
        license: "apache-2.0",
        route: PalwGenesisRouteV1::PalwQ361,
        e2e_adjudicable: true,
        tentative: false,
        kernel_version: 0,
        uncensored: true,
        out_of_scope: &["vision projector mmproj-model-f16.gguf (not fetched)", "MTP block blk.40.nextn.* (dropped by the converter)"],
        served_context: PALW_GENESIS_SERVED_CONTEXT_V1,
    },
    PalwGenesisModelV1 {
        genesis_id: 10,
        artifact_repo: "Misakachain/Llama-3.3-70B-Instruct-abliterated-PALW",
        source_repo: "huihui-ai/Llama-3.3-70B-Instruct-abliterated",
        source_revision: "fa13334669544bab573e0e5313cad629a9c02e2c",
        source_file: "",
        architecture: "LlamaForCausalLM",
        license: "llama3.3",
        route: PalwGenesisRouteV1::TirV1,
        e2e_adjudicable: false,
        tentative: false,
        kernel_version: 0,
        uncensored: true,
        out_of_scope: &[],
        served_context: PALW_GENESIS_SERVED_CONTEXT_V1,
    },
];

/// What a conversion and a class declaration fill in for one genesis model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwGenesisRootsV1 {
    /// The registered class id.
    pub class_id: Hash64,
    /// The `PALWART3` root of the model's artifact.
    pub artifact_root: Hash64,
    /// The verification plan the entry names (`0^64`: the class's own registered plan).
    pub verification_plan_id: Hash64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwGenesisCatalogErrorV1 {
    /// A genesis model the edition must list has no roots yet (its conversion or declaration has not run).
    Missing {
        genesis_id: u8,
    },
    /// Roots for an id that is not a genesis model.
    Unknown {
        genesis_id: u8,
    },
    Duplicate {
        genesis_id: u8,
    },
    /// A zero class id or artifact root (a placeholder is never a root).
    Zero {
        genesis_id: u8,
    },
    /// Two genesis models with one class id.
    SharedClass {
        genesis_id: u8,
    },
    KernelVersion {
        genesis_id: u8,
        version: u16,
    },
    Shape(&'static str),
}

/// The genesis model `id`.
pub fn palw_genesis8_model_v1(genesis_id: u8) -> Option<&'static PalwGenesisModelV1> {
    PALW_GENESIS8_MODELS_V1.iter().find(|m| m.genesis_id == genesis_id)
}

/// **Edition 0 of the next network** over the roots supplied for each genesis model. Every listed model must have roots (a tentative
/// one only when `include_tentative`; without it, a tentative model is left out and its roots, if supplied, are refused as
/// unknown so a decision is never made by accident). The result passes `AnnualModelCatalogV1::validate_shape`.
pub fn palw_genesis8_edition0_v1(
    roots: &[(u8, PalwGenesisRootsV1)],
    include_tentative: bool,
) -> Result<AnnualModelCatalogV1, PalwGenesisCatalogErrorV1> {
    use PalwGenesisCatalogErrorV1 as E;
    let listed: Vec<&PalwGenesisModelV1> = PALW_GENESIS8_MODELS_V1.iter().filter(|m| include_tentative || !m.tentative).collect();
    let mut seen: Vec<u8> = Vec::new();
    for (id, _) in roots {
        if !listed.iter().any(|m| m.genesis_id == *id) {
            return Err(E::Unknown { genesis_id: *id });
        }
        if seen.contains(id) {
            return Err(E::Duplicate { genesis_id: *id });
        }
        seen.push(*id);
    }
    let mut models = Vec::with_capacity(listed.len());
    for m in &listed {
        let (_, r) = roots.iter().find(|(id, _)| *id == m.genesis_id).ok_or(E::Missing { genesis_id: m.genesis_id })?;
        if r.class_id == Hash64::default() || r.artifact_root == Hash64::default() {
            return Err(E::Zero { genesis_id: m.genesis_id });
        }
        if m.kernel_version > PALW_ANNUAL_CATALOG_KERNEL_VERSION_MAX_V1 {
            return Err(E::KernelVersion { genesis_id: m.genesis_id, version: m.kernel_version });
        }
        if models.iter().any(|x: &ApprovedModelV1| x.class_id == r.class_id) {
            return Err(E::SharedClass { genesis_id: m.genesis_id });
        }
        models.push(ApprovedModelV1 {
            class_id: r.class_id,
            artifact_root: r.artifact_root,
            kernel_version: m.kernel_version,
            verification_plan_id: r.verification_plan_id,
        });
    }
    let catalog = AnnualModelCatalogV1::genesis(models);
    catalog.validate_shape().map_err(E::Shape)?;
    Ok(catalog)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roots_for(ids: &[u8]) -> Vec<(u8, PalwGenesisRootsV1)> {
        ids.iter()
            .map(|id| {
                (
                    *id,
                    PalwGenesisRootsV1 {
                        class_id: Hash64::from_u64_word(1000 + *id as u64),
                        artifact_root: Hash64::from_u64_word(2000 + *id as u64),
                        verification_plan_id: Hash64::default(),
                    },
                )
            })
            .collect()
    }

    #[test]
    fn the_genesis_set_is_the_users_eight_with_their_pins() {
        let ids: Vec<u8> = PALW_GENESIS8_MODELS_V1.iter().map(|m| m.genesis_id).collect();
        assert_eq!(ids, vec![1, 2, 3, 4, 5, 6, 8, 10], "7 and 9 were dropped; numbering kept");
        for m in &PALW_GENESIS8_MODELS_V1 {
            assert_eq!(m.source_revision.len(), 40, "#{}: a commit SHA", m.genesis_id);
            assert!(m.source_revision.bytes().all(|c| c.is_ascii_hexdigit()));
            assert!(m.artifact_repo.starts_with("Misakachain/"), "#{}", m.genesis_id);
        }
        let llama = palw_genesis8_model_v1(10).unwrap();
        assert!(llama.artifact_repo.trim_start_matches("Misakachain/").starts_with("Llama"), "Llama 3.3: the name begins with Llama");
        assert!(!llama.e2e_adjudicable);
        assert_eq!(PALW_GENESIS8_MODELS_V1.iter().filter(|m| m.e2e_adjudicable).count(), 7);
        assert!(palw_genesis8_model_v1(3).unwrap().tentative);
        let eight = palw_genesis8_model_v1(8).unwrap();
        assert!(
            eight.route == PalwGenesisRouteV1::PalwQ361 && eight.source_repo.starts_with("huihui-ai/"),
            "#8: rebuilt from huihui's GGUF"
        );
        assert!(PALW_GENESIS8_MODELS_V1.iter().all(|m| m.uncensored), "every genesis model is an uncensored variant");
        assert!(PALW_GENESIS8_MODELS_V1.iter().all(|m| m.served_context == 32_768), "every genesis model is served at 32K");
    }

    #[test]
    fn genesis_classes_admit_token_ids_and_refuse_every_other_input_by_name() {
        assert_eq!(palw_genesis8_admits_input_v1(PalwGenesisInputKindV1::TokenIds), Ok(()));
        for k in [
            PalwGenesisInputKindV1::Image,
            PalwGenesisInputKindV1::Audio,
            PalwGenesisInputKindV1::Video,
            PalwGenesisInputKindV1::MtpDraft,
        ] {
            assert!(palw_genesis8_admits_input_v1(k).unwrap_err().starts_with("GENESIS_SCOPE_"), "{k:?}");
        }
        assert_eq!(PALW_GENESIS8_SCOPE_V1, "text-generation");
    }

    #[test]
    fn edition_zero_is_built_only_from_supplied_roots() {
        let all = [1, 2, 3, 4, 5, 6, 8, 10];
        let c = palw_genesis8_edition0_v1(&roots_for(&all), true).expect("edition 0");
        assert_eq!((c.edition, c.activation_daa, c.previous_catalog_root), (0, 0, Hash64::default()));
        assert_eq!(c.models.len(), 8);
        assert!(c.models.windows(2).all(|w| w[0].class_id < w[1].class_id));
        // A missing model, a zero root, an unknown id, a duplicate: refused.
        assert_eq!(
            palw_genesis8_edition0_v1(&roots_for(&[1, 2, 4, 5, 6, 8, 10]), true),
            Err(PalwGenesisCatalogErrorV1::Missing { genesis_id: 3 })
        );
        let mut zero = roots_for(&all);
        zero[0].1.artifact_root = Hash64::default();
        assert_eq!(palw_genesis8_edition0_v1(&zero, true), Err(PalwGenesisCatalogErrorV1::Zero { genesis_id: 1 }));
        assert_eq!(
            palw_genesis8_edition0_v1(&roots_for(&[1, 2, 3, 4, 5, 6, 7, 8, 10]), true),
            Err(PalwGenesisCatalogErrorV1::Unknown { genesis_id: 7 })
        );
        let mut dup = roots_for(&all);
        dup.push(dup[0]);
        assert_eq!(palw_genesis8_edition0_v1(&dup, true), Err(PalwGenesisCatalogErrorV1::Duplicate { genesis_id: 1 }));
        // Without the tentative #3: seven entries, and #3's roots are refused rather than silently listed.
        let without = palw_genesis8_edition0_v1(&roots_for(&[1, 2, 4, 5, 6, 8, 10]), false).expect("seven");
        assert_eq!(without.models.len(), 7);
        assert_eq!(palw_genesis8_edition0_v1(&roots_for(&all), false), Err(PalwGenesisCatalogErrorV1::Unknown { genesis_id: 3 }));
        // The roots differ with the content: a different artifact root is a different catalog.
        let mut other = roots_for(&all);
        other[1].1.artifact_root = Hash64::from_u64_word(99);
        assert_ne!(palw_genesis8_edition0_v1(&other, true).unwrap().root(), c.root());
    }
}
