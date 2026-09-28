//! **RFC-0002 Phase F: PALW-TIR v1 on chain — the fence and the network's IR constants.**
//!
//! `Params::palw_tir_v1` is the one fence under which a class may be a PALW Canonical Tensor IR
//! program (spec [04b](../../../docs/spec/palw/04b-tensor-ir.md)) rather than a `PalwShapeProfileV3`.
//! It ships DORMANT: `None` on every preset, hashed Some-only into `consensus_params_id` and
//! `consensus_schedule_id`, collapsed whole from `Some(never())` by the identity's normaliser — the
//! `palw_heartbeat` shape of a fence with a companion value (`config::params`, ADR-0066 D1), which
//! is the only shape that keeps every shipped ruleset byte-identical and still fingerprints the value
//! where the fence is armed (`tests/palw_tir_fences_are_dormant.rs` pins the former).
//!
//! **What the value carries** (`docs/design/palw/tir/phase-f-integration.md` §2.1):
//!
//! * `prim_set_id` — BLAKE2b-512 keyed by `misaka-palw/tir-prim-set-id/v1` over the primitive-set
//!   descriptor ([`misaka_palw_tir::prim::prim_set_descriptor_v1`], spec 04b §6.0); every IR program
//!   declares it (normal form NF-1), and [`Params::validate_palw_tir_v1`] refuses a fence that names
//!   anything but THIS build's;
//! * `court_version` — the semantics version of the IR court (cone evaluation, the demand rule,
//!   PALW-TIR-33, the dissection claim form, the step-space layout);
//! * `ceilings` — the network's IR ceilings, which may only tighten the format caps of spec 04b §5.
//!
//! **`court_catalog_root` does not move** (design D2). The bundle's root is copied from
//! `palw_catalog_coverage::palw_court_catalog_root_v1` into every V2 preset's ruleset id, so it is
//! identity on every network; the IR court is a FENCED addition, exactly as
//! `palw_step_refute::KERNEL_CATALOG_FENCED_V1` is. The armed fence writes
//! [`palw_tir_court_root_v1`] — the catalog root extended by `prim_set_id` and the court version —
//! into `consensus_params_id`, so two builds whose IR courts differ have different params ids
//! exactly where the fence is armed and nowhere else.

use crate::Hash64;
use crate::config::params::{ForkActivation, PalwPostLaunchFenceV1, Params};
use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};

/// Key of [`palw_tir_prim_set_id_v1`] — the spec's (04b §6.0), exported by the IR crate.
pub const PALW_TIR_PRIM_SET_DOMAIN_V1: &[u8] = misaka_palw_tir::prim::PRIM_SET_ID_DOMAIN_V1;
/// Key of [`palw_tir_court_root_v1`].
pub const PALW_TIR_COURT_ROOT_DOMAIN_V1: &[u8] = b"misaka-palw/tir/court-root/v1";
/// The IR court's semantics version this build implements.
pub const PALW_TIR_COURT_VERSION_V1: u16 = 1;

fn keyed64(key: &[u8], parts: &[&[u8]]) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(key).to_state();
    for part in parts {
        state.update(part);
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// **The network's `prim_set_id`**: BLAKE2b-512 keyed with [`PALW_TIR_PRIM_SET_DOMAIN_V1`] over the
/// ASCII prim-set descriptor of spec 04b §6.0 — computed here, where the hashing lives, and equal to
/// the constant the IR crate's normal form requires every program to declare
/// ([`misaka_palw_tir::prim::PRIM_SET_ID_V1`]; `tests/palw_tir_v1_fence.rs` holds the two equal).
pub fn palw_tir_prim_set_id_v1() -> Hash64 {
    keyed64(PALW_TIR_PRIM_SET_DOMAIN_V1, &[&misaka_palw_tir::prim::prim_set_descriptor_v1()])
}

/// **The IR court's catalog root** — the legacy `court_catalog_root` extended by the primitive set
/// and the court version. Written into `consensus_params_id` by the armed fence (never into a
/// bundle, which would move every shipped identity).
pub fn palw_tir_court_root_v1(prim_set_id: &Hash64, court_version: u16) -> Hash64 {
    keyed64(
        PALW_TIR_COURT_ROOT_DOMAIN_V1,
        &[
            crate::palw_catalog_coverage::palw_court_catalog_root_v1().as_byte_slice(),
            prim_set_id.as_byte_slice(),
            &court_version.to_le_bytes(),
        ],
    )
}

/// **The network's IR ceilings** (design §2.1). Every field bounds a quantity admission derives
/// from a program; a network may tighten the format caps ([`Self::FORMAT_CAPS_V1`]), never widen
/// them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwTirCeilingsV1 {
    /// Canonical program bytes a registration may carry. The format's cap is 262,144 bytes; one
    /// lifecycle carrier holds about 88,000 beside the registration's other fields (design D3).
    pub max_program_bytes: u32,
    /// Nodes summed over every occurrence of one position (`pre`, each layer's block, `post`) —
    /// what bounds admission's walk and a position's node slots.
    pub max_unrolled_nodes: u32,
    /// The largest `max_context` a class layout may declare (positions a job may touch).
    pub max_context: u32,
    /// Spec 04b §8's multiply-accumulates per position at the worst case `H`.
    pub max_macs_per_position: u64,
    /// `Fixed` and `Hist` state bytes at `max_context` — a seat's working set beside the weights.
    pub max_state_bytes: u64,
    /// Peak live bytes of one position in canonical order (spec 04b §8).
    pub max_peak_live_bytes: u64,
    /// Admission's own work (spec 04b §10.3, `TirCeilingsV1::max_cone_work`): the nodes and operand
    /// refs of every commit point's cone and every `StateWrite`'s update cone, refused at this cap
    /// before it is spent. The format's cap is `2^20`; a Qwen2.5-1.5B-shaped decoder costs 905.
    pub max_cone_work: u64,
}

impl PalwTirCeilingsV1 {
    /// The widest ceilings the v1 format allows: the decoder's byte cap, `1024` layer occurrences
    /// plus `pre` and `post` at 512 nodes each, `history_bound`'s larger value, and no bound beyond
    /// the type's on the three sizes (admission and the court's own ceilings bound those).
    pub const FORMAT_CAPS_V1: Self = Self {
        max_program_bytes: misaka_palw_tir::program::MAX_PROGRAM_BYTES as u32,
        max_unrolled_nodes: ((misaka_palw_tir::program::MAX_LAYERS + 2) * misaka_palw_tir::program::MAX_NODES_PER_BLOCK) as u32,
        max_context: misaka_palw_tir::program::HISTORY_BOUND_V1_HELD,
        max_macs_per_position: u64::MAX,
        max_state_bytes: u64::MAX,
        max_peak_live_bytes: u64::MAX,
        max_cone_work: 1 << 20,
    };

    /// Is every ceiling inside the format's cap? The one check a fence value must pass.
    pub fn within_format_caps(&self) -> Result<(), &'static str> {
        let caps = Self::FORMAT_CAPS_V1;
        if self.max_program_bytes == 0 || self.max_program_bytes > caps.max_program_bytes {
            return Err("max_program_bytes must be in [1, 262,144]");
        }
        if self.max_unrolled_nodes == 0 || self.max_unrolled_nodes > caps.max_unrolled_nodes {
            return Err("max_unrolled_nodes must be in [1, 525,312]");
        }
        if self.max_context == 0 || self.max_context > caps.max_context {
            return Err("max_context must be in [1, 2^21]");
        }
        if self.max_macs_per_position == 0 || self.max_state_bytes == 0 || self.max_peak_live_bytes == 0 {
            return Err("a cost ceiling of zero admits no program");
        }
        if self.max_cone_work == 0 || self.max_cone_work > caps.max_cone_work {
            return Err("max_cone_work must be in [1, 2^20]");
        }
        Ok(())
    }

    /// The bytes both fingerprints write, in declaration order.
    fn write_into(&self, h: &mut kaspa_hashes::ConsensusParamsId) {
        h.write(self.max_program_bytes.to_le_bytes());
        h.write(self.max_unrolled_nodes.to_le_bytes());
        h.write(self.max_context.to_le_bytes());
        h.write(self.max_macs_per_position.to_le_bytes());
        h.write(self.max_state_bytes.to_le_bytes());
        h.write(self.max_peak_live_bytes.to_le_bytes());
        h.write(self.max_cone_work.to_le_bytes());
    }
}

/// **testnet-12's IR ceilings, v1 — PROVISIONAL.** The byte cap (one carrier, design D3) and the
/// cone-work cap (`2^16`: seventy times the 1.5B decoder's 905) are the lead's; the other five are
/// placeholders at the format's scale until admission v10 (Phase F step F6) measures the corpus, and
/// are fixed then. Nothing arms them: the fence is dormant.
pub const PALW_T12_TIR_CEILINGS_V1: PalwTirCeilingsV1 = PalwTirCeilingsV1 {
    max_program_bytes: 88_000,
    max_unrolled_nodes: 1 << 18,
    max_context: misaka_palw_tir::program::HISTORY_BOUND_V1_HELD,
    max_macs_per_position: 1 << 37,
    max_state_bytes: 1 << 40,
    max_peak_live_bytes: 1 << 36,
    max_cone_work: 1 << 16,
};

/// **`Params::palw_tir_v1`'s value**: the fence and what it carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwTirFenceV1 {
    pub activation: ForkActivation,
    pub prim_set_id: Hash64,
    pub court_version: u16,
    pub ceilings: PalwTirCeilingsV1,
}

impl PalwTirFenceV1 {
    /// The fence as THIS build states it: its own `prim_set_id` and court version.
    pub fn this_build_v1(activation: ForkActivation, ceilings: PalwTirCeilingsV1) -> Self {
        Self { activation, prim_set_id: palw_tir_prim_set_id_v1(), court_version: PALW_TIR_COURT_VERSION_V1, ceilings }
    }

    /// testnet-12's value at a height: this build's ids and [`PALW_T12_TIR_CEILINGS_V1`].
    pub fn testnet12_v1(activation: ForkActivation) -> Self {
        Self::this_build_v1(activation, PALW_T12_TIR_CEILINGS_V1)
    }

    /// What the fence adds to a fingerprint beside its height: the primitive set, the court
    /// version, the ceilings and the IR court root. `consensus_params_id` and
    /// `consensus_schedule_id` write these same bytes.
    pub(crate) fn write_value_into(&self, h: &mut kaspa_hashes::ConsensusParamsId) {
        h.write(self.prim_set_id.as_byte_slice());
        h.write(self.court_version.to_le_bytes());
        self.ceilings.write_into(h);
        h.write(palw_tir_court_root_v1(&self.prim_set_id, self.court_version).as_byte_slice());
    }
}

/// **The entry a testnet-12 flag-day list takes to arm the IR** — in NO list yet: the height is
/// chosen at deployment, after the Phase F drill crosses it (design §4, D-F4). One line in a list
/// arms it, through this `set`, exactly as the capacity entries wait for theirs.
pub const PALW_T12_TIR_V1_ENTRY: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_tir_v1",
    set: |params, at| {
        params.palw_tir_v1 = at.map(PalwTirFenceV1::testnet12_v1);
        params.sync_palw_tir_v1();
    },
};

impl Params {
    /// `palw_tir_v1`, resolved: `Some` only on a `ConsensusV2` network that armed it — the ONE place
    /// "may a class be an IR program here" is decided.
    pub fn palw_tir_v1_fence(&self) -> Option<PalwTirFenceV1> {
        match (&self.palw_consensus_mode, self.palw_tir_v1) {
            (PalwConsensusMode::ConsensusV2(_), Some(fence)) => Some(fence),
            _ => None,
        }
    }

    pub fn palw_tir_v1_active_at(&self, daa_score: u64) -> bool {
        self.palw_tir_v1_fence().is_some_and(|f| f.activation.is_active(daa_score))
    }

    /// **The IR fence's mirror** on the V2 bundle's state params (`PalwStateParamsV2::tir_from_daa`),
    /// which the fold reads: below the height an assembled court close carrying an IR proof reads as
    /// bytes that do not decode, as on an older build. Written here and nowhere else; `None` where the
    /// fence is not armed (or is `never()`). Call it wherever the fence is set on an assembled
    /// ruleset; [`Self::validate_palw_tir_v1`] refuses a ruleset whose copy disagrees.
    pub fn sync_palw_tir_v1(&mut self) {
        let from_daa = self.palw_tir_v1.filter(|f| f.activation != ForkActivation::never()).map(|f| f.activation.daa_score());
        if let PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_tir_from_daa(from_daa);
        }
    }

    /// **The IR fence's own refusals**, asked by [`Params::validate_palw_v2`] before a peer is dialed:
    ///
    /// * a value naming another build's `prim_set_id` or court version — a build adjudicates only
    ///   its own primitive set;
    /// * ceilings wider than the format allows;
    /// * arming on a ruleset that is not `ConsensusV2`, or that has not declared
    ///   `palw_audit_2026_09_11` — the IR's registration object is APPENDED to the lifecycle
    ///   objects, and only under that declaration does an older build skip a payload it cannot
    ///   decode instead of failing the block (A-2, `palw_lifecycle_objects_v2`);
    /// * arming without `palw_kary_court` in force at or below it — an IR class's disputes include
    ///   history dissections, which that court plays;
    /// * arming without `palw_rcore_plus` in force at or below it — an IR claim's data-availability
    ///   answers ride `MaterialDisclosedV2`, which exists only there;
    /// * a V2 bundle whose mirror of the height (`PalwStateParamsV2::tir_from_daa`) is not the fence's.
    ///
    /// A `Some(never())` value is dormant and passes (it collapses out of the identity).
    pub fn validate_palw_tir_v1(&self) -> Result<(), PalwModeV2Error> {
        let mirror = match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(bundle) => bundle.state.tir_from_daa(),
            _ => None,
        };
        let armed = self.palw_tir_v1.filter(|f| f.activation != ForkActivation::never()).map(|f| f.activation.daa_score());
        if mirror != armed {
            return Err(PalwModeV2Error::Invalid(
                "palw_tir_v1 disagrees with the V2 bundle's mirror: mirror it with Params::sync_palw_tir_v1 after the bundle is \
                 assembled",
            ));
        }
        let Some(fence) = self.palw_tir_v1 else { return Ok(()) };
        if fence.activation == ForkActivation::never() {
            return Ok(());
        }
        if fence.prim_set_id != palw_tir_prim_set_id_v1() {
            return Err(PalwModeV2Error::Invalid(
                "palw_tir_v1 names a primitive set this build does not adjudicate (prim_set_id is not this build's)",
            ));
        }
        if fence.court_version != PALW_TIR_COURT_VERSION_V1 {
            return Err(PalwModeV2Error::Invalid("palw_tir_v1 names an IR court version this build does not implement"));
        }
        fence.ceilings.within_format_caps().map_err(PalwModeV2Error::Invalid)?;
        if !matches!(self.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)) {
            return Err(PalwModeV2Error::Invalid("palw_tir_v1 is a ConsensusV2 rule: this ruleset has no V2 bundle"));
        }
        if self.palw_audit_2026_09_11.is_none() {
            return Err(PalwModeV2Error::Invalid(
                "palw_tir_v1 needs palw_audit_2026_09_11 declared: only there does an older build skip the appended IR \
                 registration object instead of failing the block that carries it (A-2)",
            ));
        }
        let court_ok =
            self.palw_kary_court.is_some_and(|k| k != ForkActivation::never() && k.daa_score() <= fence.activation.daa_score());
        if !court_ok {
            return Err(PalwModeV2Error::Invalid(
                "palw_tir_v1 needs palw_kary_court in force at or below it: an IR class's disputes include history dissections",
            ));
        }
        // An IR claim answers a data-availability demand only through `MaterialDisclosedV2`
        // (`PalwDaAnswerV1::TirEvent`), and its one-move court's convictions are R-core+'s records:
        // below `palw_rcore_plus` an IR producer could answer no DA session and would lose each by
        // silence.
        let rcore_ok =
            self.palw_rcore_plus.is_some_and(|r| r != ForkActivation::never() && r.daa_score() <= fence.activation.daa_score());
        if !rcore_ok {
            return Err(PalwModeV2Error::Invalid(
                "palw_tir_v1 needs palw_rcore_plus in force at or below it: an IR claim's data-availability answers ride \
                 MaterialDisclosedV2 (PalwDaAnswerV1::TirEvent), which exists only there",
            ));
        }
        Ok(())
    }
}
