//! **RFC-0004: the Model Improvement Protocol — the fence `palw_improvement_v1` and the protocol's
//! constants.**
//!
//! `Params::palw_improvement_v1` is the one fence under which a line may be governed by the
//! improvement protocol (RFC-0004, spec [17](../../../docs/spec/palw/17-model-improvement.md)): its
//! epochs, hard cases, datasets, candidate registry, evaluation jobs, promotion and rewards. It ships
//! DORMANT: `None` on every preset and in no testnet-12 flag-day list, hashed Some-only into
//! `consensus_params_id` and `consensus_schedule_id`, collapsed whole from `Some(never())` by the
//! identity's normaliser, its activation alone visited by `for_each_fence` — `palw_gen_v1`'s shape
//! exactly ([`crate::palw_gen_v1`]) — and `tests/palw_improvement_v1_fence.rs` pins every shipped
//! ruleset byte-identical.
//!
//! **What the value carries** (RFC-0004 *Activation plan*):
//!
//! * `scoring_set_id` — the scoring library (RFC-0004 §7.3): ExactMatch, RefLogLik, Judge, Pairwise;
//! * `sign_table_id` — the pinned integer table of binomial critical values the promotion rule reads
//!   (§7.5);
//! * `court_version` — the protocol's semantics version (epochs, promotion, rewards);
//! * `ceilings` — the network's bounds on `k_max`, `n`, the positions an epoch may evaluate, the
//!   evaluation budget's share of claim capacity, the governed lines and a policy's size.
//!
//! Validation ([`Params::validate_palw_improvement_v1`]) refuses another build's ids or version,
//! ceilings past the format's caps, a non-V2 ruleset, a ruleset without `palw_audit_2026_09_11` (A-2:
//! an older build skips the APPENDED objects), and arming without `palw_tir_v1`, `palw_gen_v1` and
//! `palw_kary_court` in force at or below it — candidates are IR classes (RFC-0002), evaluation jobs
//! are RFC-0003 pipelines, and their disputes include history dissections.
//!
//! Below the fence every improvement object (tags 70–82, [`crate::palw_state_v2::palw_object_is_improvement_v1`])
//! is dropped by name in the acceptance walk and refused by the fold as the second lock, which reads
//! the fence through the V2 bundle's mirror (`PalwStateParamsV2::improve_from_daa`, written by
//! [`Params::sync_palw_improvement_v1`] — `palw_gen_v1`'s `gen_from_daa` pattern).

use crate::Hash64;
use crate::config::params::{ForkActivation, PalwPostLaunchFenceV1, Params};
use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};

/// The improvement protocol's semantics version this build implements.
pub const PALW_IMPROVE_COURT_VERSION_V1: u16 = 1;
/// Key of [`palw_improve_scoring_set_id_v1`].
pub const PALW_IMPROVE_SCORING_SET_DOMAIN_V1: &[u8] = b"misaka-palw/improve/scoring-set/v1";
/// Key of [`palw_improve_sign_table_id_v1`].
pub const PALW_IMPROVE_SIGN_TABLE_DOMAIN_V1: &[u8] = b"misaka-palw/improve/sign-table/v1";

fn keyed64(key: &[u8], parts: &[&[u8]]) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(key).to_state();
    for part in parts {
        state.update(part);
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// **The network's `scoring_set_id`** — the scoring library this build evaluates with: the library's
/// descriptor (its version and every reference program's canonical bytes,
/// `misaka_palw_tir::scoring::scoring_set_descriptor_v1`) under this key. The IR crate is a leaf and
/// never hashes, so the key is applied here (vector: `consensus-vectors/tir-v2/scoring/set.json`).
pub fn palw_improve_scoring_set_id_v1() -> Hash64 {
    keyed64(PALW_IMPROVE_SCORING_SET_DOMAIN_V1, &[&misaka_palw_tir::scoring::scoring_set_descriptor_v1()])
}

/// **The network's `sign_table_id`** — the pinned binomial table this build's promotion reads (spec
/// 17 §17.9.2): the digest pinned in [`crate::palw_improve_promotion_v1::PALW_IMPROVE_SIGN_TABLE_ID_HEX_V1`],
/// which `the_pinned_sign_table_is_the_definition` recomputes from the whole table.
pub fn palw_improve_sign_table_id_v1() -> Hash64 {
    let hex = crate::palw_improve_promotion_v1::PALW_IMPROVE_SIGN_TABLE_ID_HEX_V1;
    let mut out = [0u8; 64];
    faster_hex::hex_decode(hex.as_bytes(), &mut out).expect("the pinned sign table id is 128 hex digits");
    Hash64::from_bytes(out)
}

/// **The network's bounds on every governed line's policy** (RFC-0004 §13). A line's policy may only
/// tighten them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwImprovementCeilingsV1 {
    /// The most candidates one epoch evaluates (`k_max`'s ceiling).
    pub max_candidates_per_epoch: u8,
    /// The most items one epoch draws (`n`'s ceiling).
    pub max_items_per_epoch: u32,
    /// The most positions one epoch's evaluation jobs may execute, over every subject and stage.
    pub max_eval_positions_per_epoch: u64,
    /// The evaluation budget's share of a span's claim capacity, in permille (ADR-0160).
    pub max_eval_budget_permille: u16,
    /// The most lines the network governs at once.
    pub max_governed_lines: u32,
    /// The largest policy an owner may sign, in bytes.
    pub max_policy_bytes: u32,
    /// The most epochs open at once across the network — with the two above, the bound on live
    /// items and results every block's root rehashes (spec 17 §17.3).
    pub max_open_epochs: u32,
}

impl PalwImprovementCeilingsV1 {
    /// The widest ceilings the format allows.
    pub const FORMAT_CAPS_V1: Self = Self {
        max_candidates_per_epoch: 16,
        max_items_per_epoch: 4_096,
        max_eval_positions_per_epoch: 1 << 40,
        max_eval_budget_permille: 1_000,
        max_governed_lines: 1_024,
        max_policy_bytes: 16_384,
        max_open_epochs: 64,
    };

    /// Is every ceiling inside the format's cap, and none zero?
    pub fn within_format_caps(&self) -> Result<(), &'static str> {
        let caps = Self::FORMAT_CAPS_V1;
        let nonzero = [
            self.max_candidates_per_epoch as u64,
            self.max_items_per_epoch as u64,
            self.max_eval_positions_per_epoch,
            self.max_eval_budget_permille as u64,
            self.max_governed_lines as u64,
            self.max_policy_bytes as u64,
            self.max_open_epochs as u64,
        ];
        if nonzero.contains(&0) {
            return Err("an improvement ceiling of zero admits no epoch");
        }
        if self.max_candidates_per_epoch > caps.max_candidates_per_epoch {
            return Err("max_candidates_per_epoch must be at most 16");
        }
        if self.max_items_per_epoch > caps.max_items_per_epoch {
            return Err("max_items_per_epoch must be at most 4,096");
        }
        if self.max_eval_positions_per_epoch > caps.max_eval_positions_per_epoch {
            return Err("max_eval_positions_per_epoch must be at most 2^40");
        }
        if self.max_eval_budget_permille > caps.max_eval_budget_permille {
            return Err("max_eval_budget_permille must be at most 1,000");
        }
        if self.max_governed_lines > caps.max_governed_lines {
            return Err("max_governed_lines must be at most 1,024");
        }
        if self.max_policy_bytes > caps.max_policy_bytes {
            return Err("max_policy_bytes must be at most 16,384");
        }
        if self.max_open_epochs > caps.max_open_epochs {
            return Err("max_open_epochs must be at most 64");
        }
        Ok(())
    }

    fn write_into(&self, h: &mut kaspa_hashes::ConsensusParamsId) {
        h.write([self.max_candidates_per_epoch]);
        h.write(self.max_items_per_epoch.to_le_bytes());
        h.write(self.max_eval_positions_per_epoch.to_le_bytes());
        h.write(self.max_eval_budget_permille.to_le_bytes());
        h.write(self.max_governed_lines.to_le_bytes());
        h.write(self.max_policy_bytes.to_le_bytes());
        h.write(self.max_open_epochs.to_le_bytes());
    }
}

/// **The drill's ceilings** — generous for the tiny classes a drill governs. Used by the drill mover
/// and the fork id's probe; no network's release carries them.
pub const PALW_DRILL_IMPROVE_CEILINGS_V1: PalwImprovementCeilingsV1 = PalwImprovementCeilingsV1 {
    max_candidates_per_epoch: 8,
    max_items_per_epoch: 1_024,
    max_eval_positions_per_epoch: 1 << 32,
    max_eval_budget_permille: 500,
    max_governed_lines: 64,
    max_policy_bytes: 16_384,
    max_open_epochs: 8,
};

/// **`Params::palw_improvement_v1`'s value**: the fence and what it carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwImprovementFenceV1 {
    pub activation: ForkActivation,
    pub scoring_set_id: Hash64,
    pub sign_table_id: Hash64,
    pub court_version: u16,
    pub ceilings: PalwImprovementCeilingsV1,
}

impl PalwImprovementFenceV1 {
    /// The fence as THIS build states it: its own ids and version.
    pub fn this_build_v1(activation: ForkActivation, ceilings: PalwImprovementCeilingsV1) -> Self {
        Self {
            activation,
            scoring_set_id: palw_improve_scoring_set_id_v1(),
            sign_table_id: palw_improve_sign_table_id_v1(),
            court_version: PALW_IMPROVE_COURT_VERSION_V1,
            ceilings,
        }
    }

    /// A drill's value at a height: this build's ids and [`PALW_DRILL_IMPROVE_CEILINGS_V1`].
    pub fn drill_v1(activation: ForkActivation) -> Self {
        Self::this_build_v1(activation, PALW_DRILL_IMPROVE_CEILINGS_V1)
    }

    /// What the fence adds to a fingerprint beside its height. `consensus_params_id` and
    /// `consensus_schedule_id` write these same bytes.
    pub(crate) fn write_value_into(&self, h: &mut kaspa_hashes::ConsensusParamsId) {
        h.write(self.scoring_set_id.as_byte_slice());
        h.write(self.sign_table_id.as_byte_slice());
        h.write(self.court_version.to_le_bytes());
        self.ceilings.write_into(h);
    }
}

/// **The entry a drill arms the improvement fence with** (`--palw-drill-improve-at`,
/// [`crate::config::drill::palw_drill_improve_fence_at_v1`]). It is in NO testnet-12 flag-day list:
/// the fence is dormant on every network.
pub const PALW_DRILL_IMPROVE_V1_ENTRY: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_improvement_v1",
    set: |params, at| {
        params.palw_improvement_v1 = at.map(PalwImprovementFenceV1::drill_v1);
        params.sync_palw_improvement_v1();
    },
};

/// The drill's one-entry list.
pub const PALW_DRILL_IMPROVE_FENCES_V1: &[PalwPostLaunchFenceV1] = &[PALW_DRILL_IMPROVE_V1_ENTRY];

impl Params {
    /// `palw_improvement_v1`, resolved: `Some` only on a `ConsensusV2` network that armed it.
    pub fn palw_improvement_v1_fence(&self) -> Option<PalwImprovementFenceV1> {
        match (&self.palw_consensus_mode, self.palw_improvement_v1) {
            (PalwConsensusMode::ConsensusV2(_), Some(fence)) => Some(fence),
            _ => None,
        }
    }

    pub fn palw_improvement_v1_active_at(&self, daa_score: u64) -> bool {
        self.palw_improvement_v1_fence().is_some_and(|f| f.activation.is_active(daa_score))
    }

    /// **The improvement fence's mirror** on the V2 bundle's state params
    /// (`PalwStateParamsV2::improve_from_daa`), which the fold and the processor read. Written here and
    /// nowhere else; `None` where the fence is not armed (or is `never()`). Call it wherever the fence is
    /// set on an assembled ruleset; [`Self::validate_palw_improvement_v1`] refuses a ruleset whose copy
    /// disagrees.
    pub fn sync_palw_improvement_v1(&mut self) {
        let armed = self.palw_improvement_v1.filter(|f| f.activation != ForkActivation::never());
        let from_daa = armed.map(|f| f.activation.daa_score());
        if let PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_improve_from_daa(from_daa).with_improve_ceilings(armed.map(|f| f.ceilings));
        }
    }

    /// **The improvement fence's own refusals**, asked by [`Params::validate_palw_v2`]. A
    /// `Some(never())` value is dormant and passes (it collapses out of the identity).
    pub fn validate_palw_improvement_v1(&self) -> Result<(), PalwModeV2Error> {
        let armed_at_all = self.palw_improvement_v1.is_some_and(|f| f.activation != ForkActivation::never());
        if armed_at_all && !matches!(self.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)) {
            return Err(PalwModeV2Error::Invalid("palw_improvement_v1 is a ConsensusV2 rule: this ruleset has no V2 bundle"));
        }
        let mirror = match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(bundle) => bundle.state.improve_from_daa().map(|at| (at, bundle.state.improve_ceilings())),
            _ => None,
        };
        let armed = self
            .palw_improvement_v1
            .filter(|f| f.activation != ForkActivation::never())
            .map(|f| (f.activation.daa_score(), Some(f.ceilings)));
        if mirror != armed {
            return Err(PalwModeV2Error::Invalid(
                "palw_improvement_v1 disagrees with the V2 bundle's mirror: mirror it with Params::sync_palw_improvement_v1 after \
                 the bundle is assembled",
            ));
        }
        let Some(fence) = self.palw_improvement_v1 else { return Ok(()) };
        if fence.activation == ForkActivation::never() {
            return Ok(());
        }
        if fence.scoring_set_id != palw_improve_scoring_set_id_v1() {
            return Err(PalwModeV2Error::Invalid(
                "palw_improvement_v1 names a scoring library this build does not evaluate with (scoring_set_id)",
            ));
        }
        if fence.sign_table_id != palw_improve_sign_table_id_v1() {
            return Err(PalwModeV2Error::Invalid(
                "palw_improvement_v1 names a sign-test table this build does not read (sign_table_id)",
            ));
        }
        if fence.court_version != PALW_IMPROVE_COURT_VERSION_V1 {
            return Err(PalwModeV2Error::Invalid("palw_improvement_v1 names a protocol version this build does not implement"));
        }
        fence.ceilings.within_format_caps().map_err(PalwModeV2Error::Invalid)?;
        if self.palw_audit_2026_09_11.is_none() {
            return Err(PalwModeV2Error::Invalid(
                "palw_improvement_v1 needs palw_audit_2026_09_11 declared: only there does an older build skip the appended \
                 improvement objects instead of failing the block that carries one (A-2)",
            ));
        }
        let at = fence.activation.daa_score();
        let tir_ok = self.palw_tir_v1.is_some_and(|t| t.activation != ForkActivation::never() && t.activation.daa_score() <= at);
        if !tir_ok {
            return Err(PalwModeV2Error::Invalid(
                "palw_improvement_v1 needs palw_tir_v1 in force at or below it: a candidate is an IR class (RFC-0002)",
            ));
        }
        let gen_ok = self.palw_gen_v1.is_some_and(|g| g.activation != ForkActivation::never() && g.activation.daa_score() <= at);
        if !gen_ok {
            return Err(PalwModeV2Error::Invalid(
                "palw_improvement_v1 needs palw_gen_v1 in force at or below it: an evaluation job is an RFC-0003 pipeline",
            ));
        }
        let fence2_ok = self.palw_tir_fence2.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= at);
        if !fence2_ok {
            return Err(PalwModeV2Error::Invalid(
                "palw_improvement_v1 needs palw_tir_fence2 in force at or below it: every evaluation pipeline is sized under H7 \
                 and no verdict may flip mid-epoch",
            ));
        }
        let kary_ok = self.palw_kary_court.is_some_and(|k| k != ForkActivation::never() && k.daa_score() <= at);
        if !kary_ok {
            return Err(PalwModeV2Error::Invalid(
                "palw_improvement_v1 needs palw_kary_court in force at or below it: evaluation disputes include history dissections",
            ));
        }
        Ok(())
    }
}
