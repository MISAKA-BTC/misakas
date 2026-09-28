//! **FP Job V5: a free-prompt job with image inputs** — the job of a vision-language class
//! (RFC-0003 §II.2.1, PALW-GEN-13), in RFC-0001's free-prompt lane. Dormant behind
//! `Params::palw_fp_job_v5`: `None` on every preset, so FP Job V4 is exactly what it was.
//!
//! **V5 embeds V4 unchanged.** [`PalwFreePromptJobV5::v4`] IS an FP Job V4 — a
//! [`PalwFreePromptJobV3`] at [`PALW_FP_V4_VERSION`] with its `DecodeConfigV4` — and every field,
//! rule and method of it is the V4 lane's own: the V4 checks run on it verbatim
//! ([`PalwFpDecodeRulesV1::check_job`]), its decoder is its `decoder_v1()`. What V5 adds is the job's
//! images, each an [`PalwGenImageInputRefV1`] (`input_root`, `h`, `w`), exactly one per image slot of
//! the class (PALW-GEN-11).
//!
//! **The wire.** `le16(8) ‖ the V4 job's bytes after its version word ‖ borsh(images)`: a V5 job is
//! a V4 job whose version word says 8 and which is followed by its images. No V4 entry point decodes
//! it as a job it admits — every V3/V4 validator refuses version 8 by name (`UnsupportedVersion`) —
//! and no V4 byte string decodes as a V5 job (version 7 is not 8). The id is the whole borsh under
//! its own key, [`fp_job_id_v5`], a domain no V3/V4 id uses.
//!
//! **One encoding per behaviour** (RFC-0003 open question 14, decided 2026-09-29): a class with image
//! slots takes V5 only; a class without them V4 only ([`palw_fp_job_version_offered_v1`]). `images`
//! is never empty, so a text-only job has exactly one encoding, V4's.
//!
//! **The image price** (RFC-0003 open question 13 — the recommendation, **PENDING USER
//! CONFIRMATION**): a V5 job's image stages are charged as prefill-equivalent prompt tokens — each
//! image slot's declared `token_equivalents`, floored by admission at `⌈admitted per-image work /
//! per-token work⌉` (`palw_gen_class_v1::palw_gen_image_token_floor_v1`) — at the job's per-token
//! price ([`palw_fp_v5_image_tokens_v1`], [`palw_fp_v5_image_charge_v1`]). Nothing in the lane reads
//! them yet: RFC-0001's D10 still credits decode leaves only, and wiring this price into the lane's
//! quanta waits for the user's confirmation.
//!
//! **The lane stays closed to V5**: a V5 claim's class resolves against the generative registry
//! ([`palw_fp_v5_resolve_class_v1`], over the `gen_classes` rows `ClassRegisteredGenV1` writes past
//! `palw_gen_v1`), but the free-prompt walk still skips every version-8 payload — the lane opens for
//! pipeline classes at its own later fence (this one, `palw_fp_job_v5`), with OQ13's price confirmed.
//! `tests/palw_fp_v5_leaves_v4_unchanged.rs` holds FP Job V4's wire, ids and validators byte for byte
//! with this module compiled in.

use std::io::{Read, Write};

use crate::Hash64;
use crate::config::params::{ForkActivation, PalwPostLaunchFenceV1, Params};
use crate::palw_freeprompt_v3::{PALW_FP_V4_VERSION, PalwFpDecodeRulesV1, PalwFpV3Error, PalwFreePromptJobV3, PalwFreePromptJobV4};
use crate::palw_gen_class_v1::{PalwGenImageInputRefV1, PalwGenJobImageErrorV1, PalwGenOffersV1, palw_gen_job_images_admitted_v1};
use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};

/// **The FP Job V5 version**: 8, the next unused job version (5 is V3, 6 ADR-0096 D8's constraint
/// job, named and unbuilt, 7 is V4). RFC-0001 may assign another number at freeze; this one is
/// provisional and binds nothing a network runs.
pub const PALW_FP_V5_VERSION: u16 = 8;
/// The V5 job id's key: a domain no V3 or V4 id uses.
pub const PALW_FP_V5_DOMAIN_JOB_ID: &[u8] = b"misaka-palw/fp-v5/job-id/v1";
/// The most images a V5 job carries: the pipeline format's image cap.
pub const PALW_FP_V5_MAX_IMAGES: usize = misaka_palw_tir::pipeline::MAX_JOB_IMAGES;

/// **FP Job V5**: an FP Job V4, unchanged, and the job's images.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwFreePromptJobV5 {
    /// The FP Job V4 the job embeds: version 7, its `DecodeConfigV4` present.
    pub v4: PalwFreePromptJobV4,
    /// One image per image slot of the class, in slot order: 1..=16.
    pub images: Vec<PalwGenImageInputRefV1>,
}

impl borsh::BorshSerialize for PalwFreePromptJobV5 {
    fn serialize<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        let v4 = borsh::to_vec(&self.v4)?;
        borsh::BorshSerialize::serialize(&PALW_FP_V5_VERSION, writer)?;
        // The V4 job's bytes after its version word (a u16): every V3 field, then its decode rules.
        writer.write_all(&v4[2..])?;
        borsh::BorshSerialize::serialize(&self.images, writer)
    }
}

impl borsh::BorshDeserialize for PalwFreePromptJobV5 {
    fn deserialize_reader<R: Read>(reader: &mut R) -> std::io::Result<Self> {
        let version: u16 = borsh::BorshDeserialize::deserialize_reader(reader)?;
        if version != PALW_FP_V5_VERSION {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, format!("version {version} is not FP Job V5")));
        }
        // What follows is a V4 job's bytes after its version word: read them as the V4 job they are,
        // with the V4 lane's own decoder, by putting the V4 version word back in front.
        let v4_word = PALW_FP_V4_VERSION.to_le_bytes();
        let mut chained = (&v4_word[..]).chain(&mut *reader);
        let v4 = PalwFreePromptJobV3::deserialize_reader(&mut chained)?;
        let images = borsh::BorshDeserialize::deserialize_reader(reader)?;
        Ok(Self { v4, images })
    }
}

/// **`fp_job_id_v5`**: `H64(key "misaka-palw/fp-v5/job-id/v1", le64(|bytes|) ‖ bytes)` over the
/// whole borsh — the V3/V4 ids' construction under V5's own key.
pub fn fp_job_id_v5(job: &PalwFreePromptJobV5) -> Hash64 {
    fp_job_id_v5_bytes(&borsh::to_vec(job).expect("an FP Job V5 is borsh-serializable"))
}

/// [`fp_job_id_v5`] of a V5 job as the lane carries it (a [`PalwFreePromptJobV3`] at version 8, its
/// images the tail) — the same bytes, so the same id; the lane's `fp_job_id_v3` dispatches here.
pub fn fp_job_id_v5_carried(job: &PalwFreePromptJobV3) -> Hash64 {
    fp_job_id_v5_bytes(&borsh::to_vec(job).expect("a free-prompt job is borsh-serializable"))
}

fn fp_job_id_v5_bytes(bytes: &[u8]) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_FP_V5_DOMAIN_JOB_ID).to_state();
    state.update(&(bytes.len() as u64).to_le_bytes());
    state.update(&bytes);
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// Why a V5 job, or a job's version for a class, is refused — each by name.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwFpV5Error {
    #[error(
        "FP Job V5 is not admitted here — it arms at a height through Params::palw_fp_job_v5, and this network is below it (or has none)"
    )]
    NotArmed,
    #[error("the job is version {version}, not FP Job V5 (version 8) with its images")]
    NotAV5Job { version: u16 },
    #[error(
        "a V5 job embeds an FP Job V4 (version 7, its decode rules present); this one embeds version {version} (decode rules present: {decode_present})"
    )]
    NotAV4Job { version: u16, decode_present: bool },
    #[error("the embedded V4 job is refused as V4 refuses it: {0}")]
    V4(PalwFpV3Error),
    #[error("a V5 job carries 1..=16 images; this one carries {0}")]
    ImageCount(usize),
    #[error(transparent)]
    Images(PalwGenJobImageErrorV1),
    #[error("{job} is not this class's job: a class with image slots takes FP Job V5 only, and a class without them FP Job V4 only")]
    JobVersionNotOffered { job: &'static str },
    #[error(
        "the prompt ({prompt}) and the decode budget ({decode}) need {need} positions; the class's text stage runs at most {max_trip}"
    )]
    ContextExceeded { prompt: u32, decode: u32, need: u64, max_trip: u32 },
    #[error("the job names class {0}, which is no generative class on this chain")]
    UnknownClass(Hash64),
    #[error("the job names a generative class of profile {profile}, which is not a text class: an FP job runs a text class only")]
    NotATextClass { profile: u8 },
    #[error("the job's tokenizer {job} is not the class's {class}")]
    TokenizerNotTheClasss { job: Hash64, class: Hash64 },
}

impl PalwFreePromptJobV5 {
    /// **A V5 job as the lane carries it**: its V4 job at version 8, the images its tail — the same
    /// bytes as this wrapper's.
    pub fn into_carried(&self) -> PalwFreePromptJobV3 {
        PalwFreePromptJobV3 { version: PALW_FP_V5_VERSION, images: Some(self.images.clone()), ..self.v4.clone() }
    }

    /// **The V5 job a carried job is**, if it is one: version 8, its decode rules and images present.
    pub fn from_carried(job: &PalwFreePromptJobV3) -> Result<Self, PalwFpV5Error> {
        let Some(images) = job.images.clone().filter(|_| job.version == PALW_FP_V5_VERSION && job.decode.is_some()) else {
            return Err(PalwFpV5Error::NotAV5Job { version: job.version });
        };
        Ok(Self { v4: PalwFreePromptJobV3 { version: PALW_FP_V4_VERSION, images: None, ..job.clone() }, images })
    }

    /// **The V5 job's own shape**: its embedded job is an FP Job V4 that V4's own rules admit, and
    /// it carries 1..=16 images.
    pub fn validate_shape_v1(&self) -> Result<(), PalwFpV5Error> {
        if self.v4.version != PALW_FP_V4_VERSION || self.v4.decode.is_none() {
            return Err(PalwFpV5Error::NotAV4Job { version: self.v4.version, decode_present: self.v4.decode.is_some() });
        }
        PalwFpDecodeRulesV1::Active.check_job(&self.v4).map_err(PalwFpV5Error::V4)?;
        if self.images.is_empty() || self.images.len() > PALW_FP_V5_MAX_IMAGES {
            return Err(PalwFpV5Error::ImageCount(self.images.len()));
        }
        Ok(())
    }
}

/// **OQ14: which job a class takes** — a class with image slots takes FP Job V5 only, and a class
/// without them FP Job V4 only. `v5` says which one is offered.
pub fn palw_fp_job_version_offered_v1(offers: &PalwGenOffersV1, v5: bool) -> Result<(), PalwFpV5Error> {
    match (offers.images.is_empty(), v5) {
        (true, false) | (false, true) => Ok(()),
        (false, false) => Err(PalwFpV5Error::JobVersionNotOffered { job: "FP Job V4" }),
        (true, true) => Err(PalwFpV5Error::JobVersionNotOffered { job: "FP Job V5" }),
    }
}

/// **A V5 job for a vision-language class** — everything checkable without chain state: the fence
/// (`armed`: `Params::palw_fp_job_v5` in force at the judged height), the job's shape, the class
/// taking V5 (OQ14), the images one per slot at its size (PALW-GEN-11), and the prompt with the
/// decode budget within the class's text stage (`prompt + decode − 1 ≤ max_trip`).
pub fn palw_fp_job_v5_admitted_v1(
    job: &PalwFreePromptJobV5,
    offers: &PalwGenOffersV1,
    text_max_trip: u32,
    armed: bool,
) -> Result<(), PalwFpV5Error> {
    if !armed {
        return Err(PalwFpV5Error::NotArmed);
    }
    job.validate_shape_v1()?;
    palw_fp_job_version_offered_v1(offers, true)?;
    palw_gen_job_images_admitted_v1(offers, &job.images).map_err(PalwFpV5Error::Images)?;
    let (prompt, decode) = (job.v4.prompt_tokens, job.v4.decode_token_limit);
    let need = (prompt as u64 + decode as u64).saturating_sub(1);
    if need > text_max_trip as u64 {
        return Err(PalwFpV5Error::ContextExceeded { prompt, decode, need, max_trip: text_max_trip });
    }
    Ok(())
}

/// **A V5 claim's class, resolved against the generative registry** (RFC-0003 §II.2.1): `row` is the
/// `gen_classes` row the job's `class_id` names (`PalwChainStateV2::gen_class_v1`). The row must
/// exist and be a text class, the job must use its tokenizer, and the class must admit the job
/// ([`palw_fp_job_v5_admitted_v1`] under the row's offers and text stage). What the lane then charges
/// is OQ13's, pending user confirmation — nothing here prices a job.
pub fn palw_fp_v5_resolve_class_v1<'a>(
    job: &PalwFreePromptJobV5,
    row: Option<&'a crate::palw_gen_class_v1::PalwGenClassRecordV1>,
    armed: bool,
) -> Result<&'a crate::palw_gen_class_v1::PalwGenClassRecordV1, PalwFpV5Error> {
    if !armed {
        return Err(PalwFpV5Error::NotArmed);
    }
    let row = row.ok_or(PalwFpV5Error::UnknownClass(job.v4.class_id))?;
    let text_max_trip = row.text_max_trip.ok_or(PalwFpV5Error::NotATextClass { profile: row.profile })?;
    if job.v4.tokenizer_id != row.tokenizer_id {
        return Err(PalwFpV5Error::TokenizerNotTheClasss { job: job.v4.tokenizer_id, class: row.tokenizer_id });
    }
    palw_fp_job_v5_admitted_v1(job, &row.class.offers, text_max_trip, armed)?;
    Ok(row)
}

/// **The image stages' price in prompt tokens** (OQ13's recommendation, PENDING USER CONFIRMATION):
/// the sum of the class's per-slot `token_equivalents` — one image per slot, so every job of the
/// class pays the same.
pub fn palw_fp_v5_image_tokens_v1(offers: &PalwGenOffersV1) -> u64 {
    offers.images.iter().map(|slot| slot.token_equivalents as u64).sum()
}

/// **The image stages' charge** (OQ13's recommendation, PENDING USER CONFIRMATION): the image tokens
/// at the job's per-token price.
pub fn palw_fp_v5_image_charge_v1(offers: &PalwGenOffersV1, per_token_price: u64) -> u128 {
    palw_fp_v5_image_tokens_v1(offers) as u128 * per_token_price as u128
}

/// **The entry a drill arms FP Job V5 with** (`--palw-drill-fp-v5-at`,
/// [`crate::config::drill::palw_drill_fp_v5_at_v1`]). In NO testnet-12 flag-day list: dormant on
/// every network.
pub const PALW_DRILL_FP_V5_ENTRY: PalwPostLaunchFenceV1 =
    PalwPostLaunchFenceV1 { name: "palw_fp_job_v5", set: |params, at| params.palw_fp_job_v5 = at };

/// The drill's one-entry list.
pub const PALW_DRILL_FP_V5_FENCES_V1: &[PalwPostLaunchFenceV1] = &[PALW_DRILL_FP_V5_ENTRY];

impl Params {
    /// `palw_fp_job_v5`, resolved: `Some` only on a `ConsensusV2` network that armed it.
    pub fn palw_fp_job_v5_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_fp_job_v5) {
            (PalwConsensusMode::ConsensusV2(_), Some(fence)) => Some(fence),
            _ => None,
        }
    }

    pub fn palw_fp_job_v5_active_at(&self, daa_score: u64) -> bool {
        self.palw_fp_job_v5_fence().is_some_and(|f| f.is_active(daa_score))
    }

    /// **FP Job V5's own refusals**, asked by [`Params::validate_palw_v2`]: a `ConsensusV2` rule, over
    /// `palw_gen_v1` (a vision-language class is a generative class) and `palw_fp_decode_rules` (V5
    /// embeds V4), both in force at or below it. A `Some(never())` value is dormant and passes.
    pub fn validate_palw_fp_job_v5_v1(&self) -> Result<(), PalwModeV2Error> {
        let Some(fence) = self.palw_fp_job_v5.filter(|f| *f != ForkActivation::never()) else { return Ok(()) };
        if !matches!(self.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)) {
            return Err(PalwModeV2Error::Invalid("palw_fp_job_v5 is armed on a network that is not ConsensusV2"));
        }
        let at_or_below =
            |other: Option<ForkActivation>| other.is_some_and(|o| o != ForkActivation::never() && o.daa_score() <= fence.daa_score());
        if !at_or_below(self.palw_gen_v1.map(|g| g.activation)) {
            return Err(PalwModeV2Error::Invalid(
                "palw_fp_job_v5 needs palw_gen_v1 in force at or below it: a vision-language class is a generative class",
            ));
        }
        if !at_or_below(self.palw_fp_decode_rules) {
            return Err(PalwModeV2Error::Invalid(
                "palw_fp_job_v5 needs palw_fp_decode_rules in force at or below it: V5 embeds FP Job V4",
            ));
        }
        Ok(())
    }
}

/// **A V5 commitment's stateless rules** (the V5 twin of the extraction walk's
/// `validate_stateless_under_ruleset_v4`): the fence, the carried job a V5 job of the right shape,
/// and every commitment rule of FP Job V4 applied to its V4 view — the same commitment with the V4 job
/// the V5 job embeds — past V4's fence. The signature is the payload's own
/// (`validate_signature_v3` over the V5 claim id), checked by the caller as for V4.
#[allow(clippy::too_many_arguments)]
pub fn palw_fp_v5_validate_payload_v1(
    payload: &crate::palw_freeprompt_v3::PalwFpCommitmentTxPayloadV3,
    network_domain: Hash64,
    panel_da_armed: bool,
    max_step_leaf_count: u64,
    ruleset_caps: Option<(u32, u32)>,
    prompt_ids_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    armed: bool,
) -> Result<PalwFreePromptJobV5, PalwFpV5Error> {
    if !armed {
        return Err(PalwFpV5Error::NotArmed);
    }
    let v5 = PalwFreePromptJobV5::from_carried(&payload.commitment.job)?;
    v5.validate_shape_v1()?;
    let mut view = payload.clone();
    view.commitment.job = v5.v4.clone();
    view.validate_stateless_under_ruleset_v4(
        network_domain,
        panel_da_armed,
        max_step_leaf_count,
        ruleset_caps,
        prompt_ids_form,
        PalwFpDecodeRulesV1::Active,
    )
    .map_err(PalwFpV5Error::V4)?;
    Ok(v5)
}
