//! **RFC-0003 §I.0: the generative job, `PalwGenJobV1`** — one job shape for every non-text profile
//! (an image, an embedding; audio and video when they are built), and its acceptance by name.
//!
//! ```text
//! PalwGenJobV1 { version: u16 = 1, envelope: PalwJobEnvelopeV1, seed: [u8; 32], body: PalwGenBodyV1 }
//! gen_job_id_v1 = H64(key "misaka-palw/gen-v1/job-id/v1", borsh(job))
//! ```
//!
//! * **One encoding per behaviour.** Acceptance verifies the canonical form and refuses every other by
//!   name ([`PalwGenJobErrorV1`]); it never rewrites a job. Canonicalisation — text to ids, a typed
//!   guidance value to its grid, an integer seed to 32 bytes — is the gateway's. An empty id list is
//!   the zero hash and a count of 0, and nothing else (a second encoding of one job is a second job
//!   id).
//! * **The body's kind equals the class's profile** ([`PalwGenJobErrorV1::BodyKindMismatch`]). **The
//!   seed is all zeros exactly when the class draws no randomness**
//!   ([`PalwGenJobErrorV1::SeedNotUsed`], [`PalwGenJobErrorV1::SeedRequired`]), so a deterministic class
//!   never has two job ids for one computation. R's key is the seed, and `image_index` is R's
//!   `position` (RFC-0003 §I.1.3): the same (class, prompt, parameters, seed, index) is the same image
//!   in any job.
//! * **The class binds the tokenizer** (Phase F D4), so the job carries none; the prompt is the user's
//!   ids without the class's template tokens, and the ids' hash is in the network's form
//!   ([`crate::palw_prompt_ids_v1`]).
//! * **What a job maps to.** Acceptance yields [`PalwGenAcceptedJobV1`]: the facts a pipeline runs
//!   over — R's item index, the step count, the job scalars in the order the class's offers declare them
//!   (an image class's `guidance_q` and the step count's POSITION among its offered counts), the id
//!   lengths and hashes, the image references — and [`palw_gen_pipeline_job_v1`] puts them, with the
//!   ids and bytes the executor holds, into the IR's `PipelineJob`.
//!
//! **The lane.** The claim that carries a generative job on chain (the commitment, its fee, its
//! signature, the bond and anchor checks of the envelope) is the free-prompt lane's, opened for
//! pipeline classes by its own later fence (RFC-0003 §Activation); nothing here is reachable from a
//! block. What this module fixes is the job itself, its identity and its acceptance against a class —
//! which the court re-reads ([`palw_gen_job_resolve_class_v1`]) from the claim's binding.

use borsh::{BorshDeserialize, BorshSerialize};

use crate::Hash64;
use crate::palw_freeprompt_v3::{PALW_FP_PRIVACY_PANEL_DA, PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_PROMPT_MODE_USER};
use crate::palw_gen_class_v1::{
    PalwGenClassRecordV1, PalwGenImageInputRefV1, PalwGenJobImageErrorV1, PalwGenProfileOffersV1, palw_gen_job_images_admitted_v1,
};
use crate::palw_gen_v1::PalwGenProfileV1;
use crate::palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_match_v1};
use crate::tx::TransactionOutpoint;
use misaka_palw_gen::OutputKindV1;
use misaka_palw_tir::pipeline::{Binding, JobImageV1, PipelineJob, TokenSource};
use misaka_palw_tir::program_v2::InputSource;

/// Wire version of [`PalwGenJobV1`].
pub const PALW_GEN_JOB_VERSION_V1: u16 = 1;
/// Key of [`palw_gen_job_id_v1`].
pub const PALW_GEN_JOB_ID_DOMAIN_V1: &[u8] = b"misaka-palw/gen-v1/job-id/v1";
/// Key of [`palw_gen_canonical_seed_v1`].
pub const PALW_GEN_CANONICAL_SEED_DOMAIN_V1: &[u8] = b"misaka-palw/gen-v1/canonical-seed/v1";

fn keyed64(key: &[u8], parts: &[&[u8]]) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(key).to_state();
    for part in parts {
        state.update(part);
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// **The envelope** — the FP V3 envelope's fields with their spec-11 meanings (RFC-0003 §I.0). The
/// lane checks them (the bond's key, the anchor's freshness, the operator); acceptance here checks the
/// network domain and the modes, which are facts of the job alone.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwJobEnvelopeV1 {
    /// The network's domain separator: a testnet job cannot certify on mainnet.
    pub network_domain: Hash64,
    /// The registered pipeline class. Programs, weights, tokenizer, templates, sampler, tables,
    /// resolution and output spec resolve THROUGH the registry row; the job carries no second copy.
    pub class_id: Hash64,
    pub executor_bond: TransactionOutpoint,
    /// MUST equal the bond record's key at the lane; carried so the signature is checkable first.
    pub executor_pubkey: Vec<u8>,
    pub operator_id: Hash64,
    pub anchor_block: Hash64,
    pub anchor_daa: u64,
    /// Uniqueness only: no lottery meaning (invariant F6). R is keyed by the seed, never by this.
    pub job_nonce: [u8; 32],
    /// [`PALW_FP_PRIVACY_PUBLIC_DA`] (the ids ride the commitment) or [`PALW_FP_PRIVACY_PANEL_DA`] (they
    /// ride the capture served to the panel). RFC-0003's decision 12: an image job defaults to
    /// `PanelDa`; `PublicDa` stays available when the user chooses it.
    pub privacy_mode: u8,
    /// [`PALW_FP_PROMPT_MODE_USER`] only in v1: the network's own canonical job has no canonical prompt
    /// for a generative class yet ([`palw_gen_canonical_seed_v1`] fixes its seed).
    pub prompt_mode: u8,
}

/// **An image job's body** (RFC-0003 §II.1.1).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwGenImageBodyV1 {
    /// The commitment of the user's prompt ids under the class tokenizer, in the network's form, WITHOUT
    /// the class's template tokens; the zero hash exactly when `prompt_tokens` is 0.
    pub prompt_token_ids_hash: Hash64,
    pub prompt_tokens: u32,
    /// The negative prompt's ids: empty unless the class offers true CFG (the zero hash and 0).
    pub negative_token_ids_hash: Hash64,
    pub negative_tokens: u32,
    /// Guidance in units of 1/16 on the class's grid; 0 on a class that offers none.
    pub guidance_q: u16,
    /// R's `position` (§I.1.3): image `k` of `n` requested images is the job with index `k`.
    pub image_index: u16,
    /// MUST equal the class's sampler descriptor id.
    pub sampler_id: Hash64,
    /// MUST be one of the class's offered step counts.
    pub steps: u16,
    /// MUST equal the class's resolution (one resolution per class in v1).
    pub width: u16,
    pub height: u16,
    /// `ImageRgb8 = 1`; nothing else in v1.
    pub output: u8,
}

/// **What an embedding job embeds.**
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwGenEmbeddingInputV1 {
    /// Text ids under the class tokenizer (hash and count, carried as FP carries a prompt).
    Text { token_ids_hash: Hash64, tokens: u32 },
    /// One canonical image (RFC-0003 §II.4): the reference only; the bytes travel with the capture.
    Image(PalwGenImageInputRefV1),
}

/// **An embedding job's body** (RFC-0003 §II.3).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwGenEmbeddingBodyV1 {
    pub input: PalwGenEmbeddingInputV1,
    /// MUST equal the class's pooling (`PALW_GEN_POOLING_*`).
    pub pooling: u8,
    /// MUST be one of the class's offered output widths.
    pub dims: u32,
    /// `EmbeddingI32 = 3`.
    pub output: u8,
}

/// **The body** — one variant per built profile. Audio and video are not built; their tags are not
/// bytes this enum decodes.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwGenBodyV1 {
    Image(PalwGenImageBodyV1),
    Embedding(PalwGenEmbeddingBodyV1),
}

impl PalwGenBodyV1 {
    /// The profile this body is a job of.
    pub fn profile(&self) -> PalwGenProfileV1 {
        match self {
            Self::Image(_) => PalwGenProfileV1::Image,
            Self::Embedding(_) => PalwGenProfileV1::Embedding,
        }
    }
}

/// **A generative job** (RFC-0003 §I.0).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwGenJobV1 {
    /// [`PALW_GEN_JOB_VERSION_V1`].
    pub version: u16,
    pub envelope: PalwJobEnvelopeV1,
    /// R's key (§I.1): all zeros for a class that draws no randomness.
    pub seed: [u8; 32],
    pub body: PalwGenBodyV1,
}

impl PalwGenJobV1 {
    /// The canonical bytes.
    pub fn encode(&self) -> Vec<u8> {
        borsh::to_vec(self).expect("a job is borsh-serializable")
    }

    /// **The unique encoding of a job**: strict Borsh (no trailing byte, every tag known), re-encoding
    /// byte-identical.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, PalwGenJobErrorV1> {
        let job: Self = borsh::from_slice(bytes).map_err(|e| PalwGenJobErrorV1::NotCanonical(e.to_string()))?;
        if job.encode() != bytes {
            return Err(PalwGenJobErrorV1::NotCanonical("re-encoding differs".into()));
        }
        Ok(job)
    }

    /// `gen_job_id_v1`.
    pub fn id(&self) -> Hash64 {
        palw_gen_job_id_v1(self)
    }

    /// The commitments of the job's text inputs, as `(prompt hash, prompt tokens, negative hash,
    /// negative tokens)`; an image embedding's (and an empty list's) are the zero hash and 0.
    pub fn text_commitments(&self) -> (Hash64, u32, Hash64, u32) {
        match &self.body {
            PalwGenBodyV1::Image(b) => (b.prompt_token_ids_hash, b.prompt_tokens, b.negative_token_ids_hash, b.negative_tokens),
            PalwGenBodyV1::Embedding(b) => match &b.input {
                PalwGenEmbeddingInputV1::Text { token_ids_hash, tokens } => (*token_ids_hash, *tokens, Hash64::default(), 0),
                PalwGenEmbeddingInputV1::Image(_) => (Hash64::default(), 0, Hash64::default(), 0),
            },
        }
    }
}

/// **`gen_job_id_v1 = H64(key "misaka-palw/gen-v1/job-id/v1", borsh(job))`** — the job's whole canonical
/// form: the envelope, the seed and the body, so no field can change after the fact.
pub fn palw_gen_job_id_v1(job: &PalwGenJobV1) -> Hash64 {
    keyed64(PALW_GEN_JOB_ID_DOMAIN_V1, &[&job.encode()])
}

/// **The network's own job's seed** (ADR-0074 D1 carried over): `H64(key
/// "misaka-palw/gen-v1/canonical-seed/v1", anchor)`, a pure function of the job's canonical anchor,
/// truncated to the seed's 32 bytes (the first half of the digest). v1 builds no canonical prompt for a
/// generative class, so no job uses it yet; the derivation is fixed now so a later one cannot differ.
pub fn palw_gen_canonical_seed_v1(anchor: &Hash64) -> [u8; 32] {
    let digest = keyed64(PALW_GEN_CANONICAL_SEED_DOMAIN_V1, &[anchor.as_byte_slice()]);
    let mut seed = [0u8; 32];
    seed.copy_from_slice(&digest.as_byte_slice()[..32]);
    seed
}

/// **Why a generative job is not the class's** — every refusal by name (RFC-0003 §II.1.1's table,
/// §I.0's rules, ADR-0096's principle: verify, never rewrite).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwGenJobErrorV1 {
    #[error("the job is not in its canonical form: {0}")]
    NotCanonical(String),
    #[error("unsupported generative job version {0}")]
    Version(u16),
    #[error("the job's network domain is not this network's")]
    NetworkDomainMismatch,
    #[error("the job names another class than the registered one")]
    ClassMismatch,
    #[error("the class's profile tag {0} takes no generative job (text jobs are FP Job V4/V5; audio and video are not built)")]
    ProfileNotAJobProfile(u8),
    #[error("a {body:?} body on a {class:?} class")]
    BodyKindMismatch { body: PalwGenProfileV1, class: PalwGenProfileV1 },
    #[error("privacy mode {0} is not PublicDa (1) or PanelDa (2)")]
    PrivacyModeNotOffered(u8),
    #[error("privacy mode 2 (PanelDa) is not armed on this network")]
    PanelDaNotArmed,
    #[error("prompt mode {0} is not the user's (0): a generative class has no canonical prompt in v1")]
    PromptModeNotOffered(u8),
    #[error("a seed on a class that draws no randomness: its seed is 32 zero bytes (two job ids for one computation)")]
    SeedNotUsed,
    #[error("a zero seed on a class that draws randomness: R's key is the job's seed, and an all-zero one is the deterministic classes' encoding")]
    SeedRequired,
    #[error("the class cannot be decoded: {0}")]
    ClassUndecodable(String),
    #[error("{what}: {tokens} ids is not 0 exactly when the hash is zero")]
    EmptyIdsEncoding { what: &'static str, tokens: u32 },
    #[error("the {what} is {tokens} ids; the class takes at most {max}")]
    PromptTooLong { what: &'static str, tokens: u32, max: u32 },
    #[error("a negative prompt on a class that offers no true CFG")]
    NegativePromptNotOffered,
    #[error("prompt id {id} at index {index} is not below the class's token bound {bound}")]
    PromptTokenOutOfRange { what: &'static str, index: usize, id: u32, bound: u32 },
    #[error("the {what}'s ids are {got} for a job that says {declared}")]
    IdsCountMismatch { what: &'static str, got: usize, declared: u32 },
    #[error("the {what}'s ids do not hash to the job's commitment")]
    IdsHashMismatch { what: &'static str },
    #[error("guidance {got} is outside the class's grid [{lo}, {hi}]")]
    GuidanceOutOfRange { got: u16, lo: u16, hi: u16 },
    #[error("the class offers no guidance: a job's guidance is 0")]
    GuidanceNotOffered,
    #[error("the sampler id is not the class's")]
    SamplerNotOffered,
    #[error("{got} steps is not one of the class's offered counts {offered:?}")]
    StepsNotOffered { got: u16, offered: Vec<u32> },
    #[error("{got_w}×{got_h} is not the class's {w}×{h} (one resolution per class)")]
    ResolutionNotOffered { got_w: u16, got_h: u16, w: u32, h: u32 },
    #[error("output kind {got} is not the one the class produces ({want})")]
    OutputSpecNotOffered { got: u8, want: u8 },
    #[error("pooling {got} is not the class's {want}")]
    PoolingNotOffered { got: u8, want: u8 },
    #[error("{got} dimensions is not one of the class's offered widths {offered:?}")]
    DimsNotOffered { got: u32, offered: Vec<u32> },
    #[error("the class does not take this input: {0}")]
    InputNotOffered(&'static str),
    #[error(transparent)]
    Image(PalwGenJobImageErrorV1),
}

/// **A job, accepted against its class**: the facts a pipeline runs over (see the module doc), none of
/// which a gateway can choose after the fact — each is a field of the job or a function of one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwGenAcceptedJobV1 {
    pub profile: PalwGenProfileV1,
    /// R's `position`: the image's index (0 for an embedding).
    pub item_index: u32,
    /// The job's step count (0 for a class that runs none).
    pub steps: u32,
    /// The job scalars in the class's declared order (`offers.scalars`).
    pub scalars: Vec<i64>,
    pub prompt_tokens: u32,
    pub prompt_hash: Hash64,
    pub negative_tokens: u32,
    pub negative_hash: Hash64,
    /// The image references the job carries (an embedding of an image).
    pub images: Vec<PalwGenImageInputRefV1>,
    /// Whether the prompt's ids ride the commitment (`PublicDa`) or the capture (`PanelDa`).
    pub public_da: bool,
}

/// **Does the class draw randomness?** Any stage declares a `Random` input.
pub fn palw_gen_class_draws_randomness_v1(programs: &[misaka_palw_tir::program_v2::TirProgramV2]) -> bool {
    programs.iter().any(|p| p.inputs.iter().any(|i| matches!(i.source, InputSource::Random { .. })))
}

/// **The job against its class's row** — everything acceptance asks that the class decides, and the
/// court re-asks of a claim's job: the version, the class, the profile, the seed rule, the modes and
/// every body field against the class's offers. (The network's domain and arming are
/// [`palw_gen_job_admitted_v1`]'s; the ids are [`palw_gen_job_ids_admitted_v1`]'s.)
pub fn palw_gen_job_resolve_class_v1(job: &PalwGenJobV1, row: &PalwGenClassRecordV1) -> Result<PalwGenAcceptedJobV1, PalwGenJobErrorV1> {
    use PalwGenJobErrorV1 as E;
    if job.version != PALW_GEN_JOB_VERSION_V1 {
        return Err(E::Version(job.version));
    }
    if job.envelope.class_id != row.class_id {
        return Err(E::ClassMismatch);
    }
    let class_profile = PalwGenProfileV1::from_tag(row.profile).ok_or(E::ProfileNotAJobProfile(row.profile))?;
    if !matches!(class_profile, PalwGenProfileV1::Image | PalwGenProfileV1::Embedding) {
        return Err(E::ProfileNotAJobProfile(row.profile));
    }
    let body_profile = job.body.profile();
    if body_profile != class_profile {
        return Err(E::BodyKindMismatch { body: body_profile, class: class_profile });
    }
    if !matches!(job.envelope.privacy_mode, PALW_FP_PRIVACY_PUBLIC_DA | PALW_FP_PRIVACY_PANEL_DA) {
        return Err(E::PrivacyModeNotOffered(job.envelope.privacy_mode));
    }
    if job.envelope.prompt_mode != PALW_FP_PROMPT_MODE_USER {
        return Err(E::PromptModeNotOffered(job.envelope.prompt_mode));
    }
    let class = &*row.class;
    let (programs, _pipeline) = class.decode().map_err(|e| E::ClassUndecodable(e.to_string()))?;
    match (palw_gen_class_draws_randomness_v1(&programs), job.seed == [0u8; 32]) {
        (true, true) => return Err(E::SeedRequired),
        (false, false) => return Err(E::SeedNotUsed),
        _ => {}
    }
    let offers = &class.offers;
    let public_da = job.envelope.privacy_mode == PALW_FP_PRIVACY_PUBLIC_DA;
    let ids_encoding = |what: &'static str, tokens: u32, hash: &Hash64| {
        if (tokens == 0) != (*hash == Hash64::default()) { Err(E::EmptyIdsEncoding { what, tokens }) } else { Ok(()) }
    };
    match (&job.body, &offers.profile) {
        (PalwGenBodyV1::Image(b), PalwGenProfileOffersV1::Image(img)) => {
            if b.output != OutputKindV1::ImageRgb8.tag() {
                return Err(E::OutputSpecNotOffered { got: b.output, want: OutputKindV1::ImageRgb8.tag() });
            }
            let (h, w) = (class.output.shape.first().copied().unwrap_or(0), class.output.shape.get(1).copied().unwrap_or(0));
            if (b.height as u32, b.width as u32) != (h, w) {
                return Err(E::ResolutionNotOffered { got_w: b.width, got_h: b.height, w, h });
            }
            if b.sampler_id != img.sampler_id {
                return Err(E::SamplerNotOffered);
            }
            let Some(position) = offers.steps.iter().position(|s| *s == b.steps as u32) else {
                return Err(E::StepsNotOffered { got: b.steps, offered: offers.steps.clone() });
            };
            ids_encoding("prompt", b.prompt_tokens, &b.prompt_token_ids_hash)?;
            ids_encoding("negative prompt", b.negative_tokens, &b.negative_token_ids_hash)?;
            if b.prompt_tokens > offers.max_prompt_tokens {
                return Err(E::PromptTooLong { what: "prompt", tokens: b.prompt_tokens, max: offers.max_prompt_tokens });
            }
            if b.negative_tokens > 0 && offers.max_negative_tokens == 0 {
                return Err(E::NegativePromptNotOffered);
            }
            if b.negative_tokens > offers.max_negative_tokens {
                return Err(E::PromptTooLong { what: "negative prompt", tokens: b.negative_tokens, max: offers.max_negative_tokens });
            }
            match &img.guidance {
                Some(g) if b.guidance_q < g.lo || b.guidance_q > g.hi => {
                    return Err(E::GuidanceOutOfRange { got: b.guidance_q, lo: g.lo, hi: g.hi });
                }
                None if b.guidance_q != 0 => return Err(E::GuidanceNotOffered),
                _ => {}
            }
            let mut scalars = vec![0i64; offers.scalars.len()];
            if let Some(g) = &img.guidance {
                scalars[g.scalar as usize] = b.guidance_q as i64;
            }
            if let Some(i) = img.steps_scalar {
                scalars[i as usize] = position as i64;
            }
            Ok(PalwGenAcceptedJobV1 {
                profile: PalwGenProfileV1::Image,
                item_index: b.image_index as u32,
                steps: b.steps as u32,
                scalars,
                prompt_tokens: b.prompt_tokens,
                prompt_hash: b.prompt_token_ids_hash,
                negative_tokens: b.negative_tokens,
                negative_hash: b.negative_token_ids_hash,
                images: Vec::new(),
                public_da,
            })
        }
        (PalwGenBodyV1::Embedding(b), PalwGenProfileOffersV1::Embedding(e)) => {
            if b.output != OutputKindV1::EmbeddingI32.tag() {
                return Err(E::OutputSpecNotOffered { got: b.output, want: OutputKindV1::EmbeddingI32.tag() });
            }
            if b.pooling != e.pooling {
                return Err(E::PoolingNotOffered { got: b.pooling, want: e.pooling });
            }
            if !e.dims.contains(&b.dims) {
                return Err(E::DimsNotOffered { got: b.dims, offered: e.dims.clone() });
            }
            let (prompt_tokens, prompt_hash, images) = match &b.input {
                PalwGenEmbeddingInputV1::Text { token_ids_hash, tokens } => {
                    if offers.max_prompt_tokens == 0 || !offers.images.is_empty() {
                        return Err(E::InputNotOffered("the class does not embed text"));
                    }
                    if *tokens == 0 {
                        return Err(E::InputNotOffered("an empty text embeds nothing"));
                    }
                    ids_encoding("text", *tokens, token_ids_hash)?;
                    if *tokens > offers.max_prompt_tokens {
                        return Err(E::PromptTooLong { what: "text", tokens: *tokens, max: offers.max_prompt_tokens });
                    }
                    (*tokens, *token_ids_hash, Vec::new())
                }
                PalwGenEmbeddingInputV1::Image(reference) => {
                    if offers.max_prompt_tokens != 0 || offers.images.len() != 1 {
                        return Err(E::InputNotOffered("the class does not embed one image"));
                    }
                    palw_gen_job_images_admitted_v1(offers, std::slice::from_ref(reference)).map_err(E::Image)?;
                    (0, Hash64::default(), vec![*reference])
                }
            };
            Ok(PalwGenAcceptedJobV1 {
                profile: PalwGenProfileV1::Embedding,
                item_index: 0,
                steps: 0,
                scalars: Vec::new(),
                prompt_tokens,
                prompt_hash,
                negative_tokens: 0,
                negative_hash: Hash64::default(),
                images,
                public_da,
            })
        }
        // A class whose profile offers do not match its profile is refused at registration; a row that
        // got here with one is not a class this job can be accepted against.
        _ => Err(E::InputNotOffered("the class's profile offers are not its profile's")),
    }
}

/// **A job admitted by a network**: its domain and the privacy mode's arming, then the class's
/// ([`palw_gen_job_resolve_class_v1`]). `panel_da_armed` is `Params::palw_panel_da_at` at the judged
/// height.
pub fn palw_gen_job_admitted_v1(
    job: &PalwGenJobV1,
    row: &PalwGenClassRecordV1,
    network_domain: Hash64,
    panel_da_armed: bool,
) -> Result<PalwGenAcceptedJobV1, PalwGenJobErrorV1> {
    if job.envelope.network_domain != network_domain {
        return Err(PalwGenJobErrorV1::NetworkDomainMismatch);
    }
    if job.envelope.privacy_mode == PALW_FP_PRIVACY_PANEL_DA && !panel_da_armed {
        return Err(PalwGenJobErrorV1::PanelDaNotArmed);
    }
    palw_gen_job_resolve_class_v1(job, row)
}

/// The ids a job's text inputs are, as an executor, a seat or a court holds them.
#[derive(Clone, Copy, Debug, Default)]
pub struct PalwGenIdsV1<'a> {
    pub prompt: &'a [u32],
    pub negative: &'a [u32],
}

/// **The largest id a class's stages read a source's ids as, plus one**: the least `token_bound` of the
/// stages whose token run reads the source, and the least `hi + 1` of the inputs a job-token binding
/// feeds it to. 0 when no stage reads the source.
pub fn palw_gen_token_bound_v1(row: &PalwGenClassRecordV1, source: TokenSource) -> Result<u32, PalwGenJobErrorV1> {
    let (programs, pipeline) = row.class.decode().map_err(|e| PalwGenJobErrorV1::ClassUndecodable(e.to_string()))?;
    let mut bound: Option<u32> = None;
    let mut lower = |b: u32| bound = Some(bound.map_or(b, |x| x.min(b)));
    for st in &pipeline.stages {
        let program = &programs[st.program as usize];
        if st.tokens.as_ref().is_some_and(|r| r.source == source) {
            lower(program.token_bound);
        }
        let externals: Vec<_> = program.inputs.iter().filter(|d| d.is_external()).collect();
        for (b, d) in st.bind.iter().zip(externals) {
            if let Binding::JobTokens { rule } = b
                && rule.source == source
            {
                let (_, hi) = d.interval();
                lower(u32::try_from(hi.saturating_add(1)).unwrap_or(u32::MAX));
            }
        }
    }
    Ok(bound.unwrap_or(0))
}

/// **A job's ids against the job**: each list as long as the job says, hashing to the job's commitment
/// in the network's form, and every id below the bound the class's stages read it under
/// ([`PalwGenJobErrorV1::PromptTokenOutOfRange`]).
pub fn palw_gen_job_ids_admitted_v1(
    row: &PalwGenClassRecordV1,
    accepted: &PalwGenAcceptedJobV1,
    ids: PalwGenIdsV1<'_>,
    form: PalwPromptIdsFormV1,
) -> Result<(), PalwGenJobErrorV1> {
    use PalwGenJobErrorV1 as E;
    for (what, source, held, declared, hash) in [
        ("prompt", TokenSource::Prompt, ids.prompt, accepted.prompt_tokens, accepted.prompt_hash),
        ("negative prompt", TokenSource::Negative, ids.negative, accepted.negative_tokens, accepted.negative_hash),
    ] {
        if held.len() != declared as usize {
            return Err(E::IdsCountMismatch { what, got: held.len(), declared });
        }
        if declared == 0 {
            continue;
        }
        if !prompt_token_ids_match_v1(form, held, &hash) {
            return Err(E::IdsHashMismatch { what });
        }
        let bound = palw_gen_token_bound_v1(row, source)?;
        if let Some((index, id)) = held.iter().enumerate().find(|(_, id)| **id >= bound) {
            return Err(E::PromptTokenOutOfRange { what, index, id: *id, bound });
        }
    }
    Ok(())
}

/// **The IR's job** an accepted job runs over: R's draws are the executor's seed and the item index
/// (`PalwGenDrawV1`), the rest is this.
pub fn palw_gen_pipeline_job_v1(accepted: &PalwGenAcceptedJobV1, ids: PalwGenIdsV1<'_>, images: Vec<JobImageV1>) -> PipelineJob {
    PipelineJob {
        prompt: ids.prompt.to_vec(),
        negative: ids.negative.to_vec(),
        steps: accepted.steps,
        scalars: accepted.scalars.clone(),
        images,
        generated: Vec::new(),
        source: Vec::new(),
    }
}
