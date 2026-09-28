//! **RFC-0003: generative model classes on chain — the fence `palw_gen_v1` and the network's
//! generative constants.**
//!
//! `Params::palw_gen_v1` is the one fence under which a class may be a PIPELINE of PALW-TIR version-2
//! programs (spec [04b](../../../docs/spec/palw/04b-tensor-ir.md) §15) — an image, embedding, audio
//! or video class (RFC-0003 Part II); text keeps RFC-0001's FP lane. It ships DORMANT: `None` on every
//! preset and in no testnet-12 flag-day list, hashed Some-only into `consensus_params_id` and
//! `consensus_schedule_id`, collapsed whole from `Some(never())` by the identity's normaliser, its
//! activation alone visited by `for_each_fence` — `palw_tir_v1`'s shape exactly
//! ([`crate::palw_tir_v1`]), and `tests/palw_gen_fences_are_dormant.rs` pins every shipped ruleset
//! byte-identical.
//!
//! **What the value carries** (RFC-0003 §Activation):
//!
//! * `program_version` — 2 (`TirProgramV2`); `prim_set_id` — the primitive set, which version 2 does
//!   not change (PALW-TIR-41);
//! * `rand_set_id` — RFC-0003 §I.1's randomness (the domain table and both tables' pins,
//!   `misaka_palw_gen::rand::rand_set_id_v1`); `output_set_id` — §I.3's canonical outputs
//!   (`misaka_palw_gen::output::output_set_id_v1`);
//! * `court_version` — the generative court's semantics (derived inputs, stage edges, the output
//!   digest);
//! * `ceilings` — one set PER PROFILE (the user's decision 5 of 2026-09-28).
//!
//! Validation ([`Params::validate_palw_gen_v1`]) refuses another build's ids or versions, ceilings
//! past the format's caps, a non-V2 ruleset, a ruleset without `palw_audit_2026_09_11` (A-2: an older
//! build skips the APPENDED registration object), and arming without `palw_tir_v1` in force at or
//! below it — the generative court is the IR court with version 2's additions.
//!
//! Until the pipeline class's admission is wired, the registration object is dropped by name at every
//! height ([`crate::palw_state_v2::palw_object_is_gen_v1`]), so the fence arms nothing a block can
//! carry yet — which is why it needs no bundle mirror (`palw_tir_v1`'s `tir_from_daa`) in this step.

use crate::Hash64;
use crate::config::params::{ForkActivation, PalwPostLaunchFenceV1, Params};
use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};

/// Key of [`palw_gen_court_root_v1`].
pub const PALW_GEN_COURT_ROOT_DOMAIN_V1: &[u8] = b"misaka-palw/gen/court-root/v1";
/// The program version a generative class's stages are (spec 04b §15.1).
pub const PALW_GEN_PROGRAM_VERSION_V1: u16 = misaka_palw_tir::program_v2::TIR_PROGRAM_VERSION_V2;
/// The generative court's semantics version this build implements.
pub const PALW_GEN_COURT_VERSION_V1: u16 = 1;

fn keyed64(key: &[u8], parts: &[&[u8]]) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(key).to_state();
    for part in parts {
        state.update(part);
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// **The network's `rand_set_id`** — RFC-0003 §I.1's domain table and table pins.
pub fn palw_gen_rand_set_id_v1() -> Hash64 {
    Hash64::from_bytes(misaka_palw_gen::rand::rand_set_id_v1())
}

/// **The network's `output_set_id`** — RFC-0003 §I.3's output kinds and digest construction.
pub fn palw_gen_output_set_id_v1() -> Hash64 {
    Hash64::from_bytes(misaka_palw_gen::output::output_set_id_v1())
}

/// **The generative court's root**: the IR court root ([`crate::palw_tir_v1::palw_tir_court_root_v1`],
/// the catalog root extended by the primitive set and the IR court version) extended by the program
/// version, the rand set, the output set and the generative court version. Written into
/// `consensus_params_id` by the armed fence.
pub fn palw_gen_court_root_v1(
    prim_set_id: &Hash64,
    program_version: u16,
    rand_set_id: &Hash64,
    output_set_id: &Hash64,
    court_version: u16,
) -> Hash64 {
    let tir = crate::palw_tir_v1::palw_tir_court_root_v1(prim_set_id, crate::palw_tir_v1::PALW_TIR_COURT_VERSION_V1);
    keyed64(
        PALW_GEN_COURT_ROOT_DOMAIN_V1,
        &[
            tir.as_byte_slice(),
            &program_version.to_le_bytes(),
            rand_set_id.as_byte_slice(),
            output_set_id.as_byte_slice(),
            &court_version.to_le_bytes(),
        ],
    )
}

/// The generative profiles (RFC-0003 Part II). Text is not one: it keeps RFC-0001's FP lane.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum PalwGenProfileV1 {
    Image = 1,
    Embedding = 2,
    Audio = 3,
    Video = 4,
}

impl PalwGenProfileV1 {
    pub const ALL: [PalwGenProfileV1; 4] =
        [PalwGenProfileV1::Image, PalwGenProfileV1::Embedding, PalwGenProfileV1::Audio, PalwGenProfileV1::Video];

    pub fn from_tag(tag: u8) -> Option<Self> {
        Self::ALL.iter().copied().find(|p| *p as u8 == tag)
    }

    /// The output kind a class of the profile must produce (RFC-0003 §I.3.3).
    pub const fn output_kind(self) -> misaka_palw_gen::OutputKindV1 {
        match self {
            PalwGenProfileV1::Image => misaka_palw_gen::OutputKindV1::ImageRgb8,
            PalwGenProfileV1::Embedding => misaka_palw_gen::OutputKindV1::EmbeddingI32,
            PalwGenProfileV1::Audio => misaka_palw_gen::OutputKindV1::PcmI16,
            PalwGenProfileV1::Video => misaka_palw_gen::OutputKindV1::VideoRgb8,
        }
    }
}

/// **One profile's ceilings.** Per position they bound each stage (spec 04b §15.9 with the IR
/// court's terminal ceilings); per job they bound the pipeline's totals; per class they bound what a
/// registration carries (several carriers: the user's decision 8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwGenProfileCeilingsV1 {
    /// Multiply-accumulates of one position of one stage.
    pub max_position_macs: u64,
    /// Step leaves one position of one stage commits.
    pub max_position_step_leaves: u64,
    /// `Fixed` and `Hist` state bytes of one stage's run.
    pub max_state_bytes: u64,
    /// `Σ` over stages of `max_trip ×` the per-position MACs.
    pub max_job_macs: u64,
    pub max_job_transcendentals: u64,
    /// `Σ` over stages of `max_trip ×` the per-position step leaves.
    pub max_job_step_leaves: u64,
    /// Admission's own work over every stage.
    pub max_job_cone_work: u64,
    /// Stages a class may have.
    pub max_stages: u8,
    /// Bytes of the pipeline and every program a registration carries.
    pub max_class_bytes: u32,
}

impl PalwGenProfileCeilingsV1 {
    /// The widest ceilings the format allows: sixteen stages of the program cap and the pipeline's,
    /// `2^26` leaves a position, the ladder's `2^32` leaves a job, and `2^20` of admission work a stage.
    pub const FORMAT_CAPS_V1: Self = Self {
        max_position_macs: u64::MAX,
        max_position_step_leaves: 1 << 26,
        max_state_bytes: u64::MAX,
        max_job_macs: u64::MAX,
        max_job_transcendentals: u64::MAX,
        max_job_step_leaves: crate::palw_context_ladder::PALW_CONTEXT_LADDER_MAX_STEP_LEAVES,
        max_job_cone_work: (misaka_palw_tir::pipeline::MAX_STAGES as u64) << 20,
        max_stages: misaka_palw_tir::pipeline::MAX_STAGES as u8,
        max_class_bytes: (misaka_palw_tir::pipeline::MAX_STAGES * misaka_palw_tir::program::MAX_PROGRAM_BYTES
            + misaka_palw_tir::pipeline::MAX_PIPELINE_BYTES) as u32,
    };

    /// Is every ceiling inside the format's cap, and none zero?
    pub fn within_format_caps(&self) -> Result<(), &'static str> {
        let caps = Self::FORMAT_CAPS_V1;
        let nonzero = [
            self.max_position_macs,
            self.max_position_step_leaves,
            self.max_state_bytes,
            self.max_job_macs,
            self.max_job_transcendentals,
            self.max_job_step_leaves,
            self.max_job_cone_work,
            self.max_stages as u64,
            self.max_class_bytes as u64,
        ];
        if nonzero.contains(&0) {
            return Err("a generative ceiling of zero admits no class");
        }
        if self.max_position_step_leaves > caps.max_position_step_leaves {
            return Err("max_position_step_leaves must be at most 2^26");
        }
        if self.max_job_step_leaves > caps.max_job_step_leaves {
            return Err("max_job_step_leaves must be at most the ladder's 2^32");
        }
        if self.max_job_cone_work > caps.max_job_cone_work {
            return Err("max_job_cone_work must be at most 16 · 2^20");
        }
        if self.max_stages > caps.max_stages {
            return Err("max_stages must be at most 16");
        }
        if self.max_class_bytes > caps.max_class_bytes {
            return Err("max_class_bytes must be at most sixteen programs and a pipeline");
        }
        Ok(())
    }

    fn write_into(&self, h: &mut kaspa_hashes::ConsensusParamsId) {
        h.write(self.max_position_macs.to_le_bytes());
        h.write(self.max_position_step_leaves.to_le_bytes());
        h.write(self.max_state_bytes.to_le_bytes());
        h.write(self.max_job_macs.to_le_bytes());
        h.write(self.max_job_transcendentals.to_le_bytes());
        h.write(self.max_job_step_leaves.to_le_bytes());
        h.write(self.max_job_cone_work.to_le_bytes());
        h.write([self.max_stages]);
        h.write(self.max_class_bytes.to_le_bytes());
    }
}

/// **The ceilings of every profile** (the user's decision 5: per-profile ceilings).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwGenCeilingsV1 {
    pub image: PalwGenProfileCeilingsV1,
    pub embedding: PalwGenProfileCeilingsV1,
    pub audio: PalwGenProfileCeilingsV1,
    pub video: PalwGenProfileCeilingsV1,
}

impl PalwGenCeilingsV1 {
    pub fn of(&self, profile: PalwGenProfileV1) -> &PalwGenProfileCeilingsV1 {
        match profile {
            PalwGenProfileV1::Image => &self.image,
            PalwGenProfileV1::Embedding => &self.embedding,
            PalwGenProfileV1::Audio => &self.audio,
            PalwGenProfileV1::Video => &self.video,
        }
    }

    pub fn within_format_caps(&self) -> Result<(), &'static str> {
        for p in PalwGenProfileV1::ALL {
            self.of(p).within_format_caps()?;
        }
        Ok(())
    }

    fn write_into(&self, h: &mut kaspa_hashes::ConsensusParamsId) {
        for p in PalwGenProfileV1::ALL {
            h.write([p as u8]);
            self.of(p).write_into(h);
        }
    }
}

/// **The drill's generative ceilings** — generous for the fixtures a drill registers (tiny
/// pipelines), provisional until the first profile's corpus is measured. Used by the drill mover and
/// the fork id's probe; no network's release carries them.
pub const PALW_DRILL_GEN_PROFILE_CEILINGS_V1: PalwGenProfileCeilingsV1 = PalwGenProfileCeilingsV1 {
    max_position_macs: 1 << 40,
    max_position_step_leaves: 1 << 22,
    max_state_bytes: 1 << 34,
    max_job_macs: 1 << 46,
    max_job_transcendentals: 1 << 40,
    max_job_step_leaves: 1 << 30,
    max_job_cone_work: 1 << 22,
    max_stages: 16,
    max_class_bytes: 1 << 21,
};

pub const PALW_DRILL_GEN_CEILINGS_V1: PalwGenCeilingsV1 = PalwGenCeilingsV1 {
    image: PALW_DRILL_GEN_PROFILE_CEILINGS_V1,
    embedding: PALW_DRILL_GEN_PROFILE_CEILINGS_V1,
    audio: PALW_DRILL_GEN_PROFILE_CEILINGS_V1,
    video: PALW_DRILL_GEN_PROFILE_CEILINGS_V1,
};

/// **`Params::palw_gen_v1`'s value**: the fence and what it carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwGenFenceV1 {
    pub activation: ForkActivation,
    pub program_version: u16,
    pub prim_set_id: Hash64,
    pub rand_set_id: Hash64,
    pub output_set_id: Hash64,
    pub court_version: u16,
    pub ceilings: PalwGenCeilingsV1,
}

impl PalwGenFenceV1 {
    /// The fence as THIS build states it: its own ids and versions.
    pub fn this_build_v1(activation: ForkActivation, ceilings: PalwGenCeilingsV1) -> Self {
        Self {
            activation,
            program_version: PALW_GEN_PROGRAM_VERSION_V1,
            prim_set_id: crate::palw_tir_v1::palw_tir_prim_set_id_v1(),
            rand_set_id: palw_gen_rand_set_id_v1(),
            output_set_id: palw_gen_output_set_id_v1(),
            court_version: PALW_GEN_COURT_VERSION_V1,
            ceilings,
        }
    }

    /// A drill's value at a height: this build's ids and [`PALW_DRILL_GEN_CEILINGS_V1`].
    pub fn drill_v1(activation: ForkActivation) -> Self {
        Self::this_build_v1(activation, PALW_DRILL_GEN_CEILINGS_V1)
    }

    /// What the fence adds to a fingerprint beside its height. `consensus_params_id` and
    /// `consensus_schedule_id` write these same bytes.
    pub(crate) fn write_value_into(&self, h: &mut kaspa_hashes::ConsensusParamsId) {
        h.write(self.program_version.to_le_bytes());
        h.write(self.prim_set_id.as_byte_slice());
        h.write(self.rand_set_id.as_byte_slice());
        h.write(self.output_set_id.as_byte_slice());
        h.write(self.court_version.to_le_bytes());
        self.ceilings.write_into(h);
        h.write(
            palw_gen_court_root_v1(&self.prim_set_id, self.program_version, &self.rand_set_id, &self.output_set_id, self.court_version)
                .as_byte_slice(),
        );
    }
}

/// **The entry a drill arms the generative fence with** (`--palw-drill-gen-at`,
/// [`crate::config::drill::palw_drill_gen_fence_at_v1`]). It is in NO testnet-12 flag-day list: the
/// fence is dormant on every network (the coordinator's rule for this step).
pub const PALW_DRILL_GEN_V1_ENTRY: PalwPostLaunchFenceV1 =
    PalwPostLaunchFenceV1 { name: "palw_gen_v1", set: |params, at| params.palw_gen_v1 = at.map(PalwGenFenceV1::drill_v1) };

/// The drill's one-entry list.
pub const PALW_DRILL_GEN_FENCES_V1: &[PalwPostLaunchFenceV1] = &[PALW_DRILL_GEN_V1_ENTRY];

impl Params {
    /// `palw_gen_v1`, resolved: `Some` only on a `ConsensusV2` network that armed it.
    pub fn palw_gen_v1_fence(&self) -> Option<PalwGenFenceV1> {
        match (&self.palw_consensus_mode, self.palw_gen_v1) {
            (PalwConsensusMode::ConsensusV2(_), Some(fence)) => Some(fence),
            _ => None,
        }
    }

    pub fn palw_gen_v1_active_at(&self, daa_score: u64) -> bool {
        self.palw_gen_v1_fence().is_some_and(|f| f.activation.is_active(daa_score))
    }

    /// **The generative fence's own refusals**, asked by [`Params::validate_palw_v2`]. A
    /// `Some(never())` value is dormant and passes (it collapses out of the identity).
    pub fn validate_palw_gen_v1(&self) -> Result<(), PalwModeV2Error> {
        let Some(fence) = self.palw_gen_v1 else { return Ok(()) };
        if fence.activation == ForkActivation::never() {
            return Ok(());
        }
        if fence.program_version != PALW_GEN_PROGRAM_VERSION_V1 {
            return Err(PalwModeV2Error::Invalid("palw_gen_v1 names a program version this build does not implement"));
        }
        if fence.prim_set_id != crate::palw_tir_v1::palw_tir_prim_set_id_v1() {
            return Err(PalwModeV2Error::Invalid(
                "palw_gen_v1 names a primitive set this build does not adjudicate (prim_set_id is not this build's)",
            ));
        }
        if fence.rand_set_id != palw_gen_rand_set_id_v1() {
            return Err(PalwModeV2Error::Invalid("palw_gen_v1 names a randomness set this build does not compute (rand_set_id)"));
        }
        if fence.output_set_id != palw_gen_output_set_id_v1() {
            return Err(PalwModeV2Error::Invalid("palw_gen_v1 names an output set this build does not encode (output_set_id)"));
        }
        if fence.court_version != PALW_GEN_COURT_VERSION_V1 {
            return Err(PalwModeV2Error::Invalid("palw_gen_v1 names a generative court version this build does not implement"));
        }
        fence.ceilings.within_format_caps().map_err(PalwModeV2Error::Invalid)?;
        if !matches!(self.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)) {
            return Err(PalwModeV2Error::Invalid("palw_gen_v1 is a ConsensusV2 rule: this ruleset has no V2 bundle"));
        }
        if self.palw_audit_2026_09_11.is_none() {
            return Err(PalwModeV2Error::Invalid(
                "palw_gen_v1 needs palw_audit_2026_09_11 declared: only there does an older build skip the appended \
                 generative registration object instead of failing the block that carries it (A-2)",
            ));
        }
        let tir_ok = self
            .palw_tir_v1
            .is_some_and(|t| t.activation != ForkActivation::never() && t.activation.daa_score() <= fence.activation.daa_score());
        if !tir_ok {
            return Err(PalwModeV2Error::Invalid(
                "palw_gen_v1 needs palw_tir_v1 in force at or below it: the generative court is the IR court with program \
                 version 2's additions",
            ));
        }
        Ok(())
    }
}
