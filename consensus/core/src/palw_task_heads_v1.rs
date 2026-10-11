//! **The task-head profile (`Head`) — the fence `palw_task_heads_v1`** (HFX, 2026-10-08; design
//! `docs/design/palw/tir/task-heads-profile-v1.md`; allocation approved by the Lead 2026-10-08: the fence, the profile tag 6, the
//! appended offers variant 3 and body variant 2).
//!
//! A `Head` class is an RFC-0003 generative class (`PalwGenClassV1`, carried by `ClassRegisteredGenV1`, tag 68, unchanged) whose job is a
//! complete task over a classifier, a token or span head, a masked-LM head or a vision head: its canonical output is ONE tensor of
//! unnormalised logits, `EmbeddingI32 [rows, labels]` (no new output kind: `output_set_id` and the armed `palw_gen_v1` fingerprint do not
//! move), and its offers ([`PalwGenHeadOffersV1`]) bind the task, the label map's root, a pair's separator, the zero-shot entailment label
//! and the masked row's job scalar. The court judges the output tensor; the label is a pure function of the verified output and the
//! offers ([`palw_head_decode_v1`], versioned into the fence's `head_set_id`), so a verified output has exactly one answer.
//!
//! **Dormant**: `None` on every preset and in no flag-day list; hashed Some-only into `consensus_params_id` and `consensus_schedule_id`,
//! collapsed whole from `Some(never())`, its activation alone visited by `for_each_fence`. **This build refuses arming it**
//! ([`Params::validate_palw_task_heads_v1`]): it is one of the fences the single future full-activation release arms, after every RFC is
//! implemented. What that release will additionally require is [`Params::palw_task_heads_v1_arming_preconditions`] (`palw_gen_v1` in
//! force at or below it, the bundle's mirror, this build's head set, the ceilings inside the format's caps). Tests and the census arm it
//! on a `Params` they build directly (`PALW_DRILL_TASK_HEADS_ENTRY`), never through validation.
//!
//! **The A-2 split class (the Lead's critical condition).** `palw_gen_v1` is armed on testnet-12, so the int-12 build decodes tag 68 and
//! FP job version 10 — but NOT the appended variants: a class whose offers are `Head` (variant 3) or a job whose body is `Head`
//! (variant 2) is bytes int-12 cannot decode. int-12 then (a) tolerates the lifecycle carrier at isolation on a ruleset that declared
//! `palw_audit_2026_09_11` and refuses it elsewhere, (b) skips it at extraction (no slot, no rent, no budget, no state), (c) reads an
//! assembled chunk group carrying it as undecodable, and (d) refuses a version-10 free-prompt payload carrying it at the isolation door
//! (the block is invalid). Below this fence this build does EXACTLY that, with [`palw_object_needs_task_heads_v1`] as the one predicate:
//! (a) `validate_palw_lifecycle_tx` treats a decoded object that needs the fence as undecodable; (b) the acceptance walk drops it by
//! name first and charges nothing, and the fold refuses it as the second lock; (c) both chunk assemblies read it as undecodable; (d) the
//! header-context door refuses the payload ([`palw_fp_head_job_refusal_at_v1`]): the block is invalid on both builds. A class whose
//! PROFILE BYTE is 6 but whose offers are an older variant is bytes int-12 decodes, so it is NOT dropped: it takes int-12's own path,
//! refused at admission as an unknown profile (`PalwGenClassErrorV1::Profile(6)`), because below the fence this build resolves profile
//! tags exactly as int-12 does ([`crate::palw_gen_v1::PalwGenProfileV1::from_tag`]).

use borsh::{BorshDeserialize, BorshSerialize};

use crate::Hash64;
use crate::config::params::{ForkActivation, PalwPostLaunchFenceV1, Params};
use crate::palw_gen_class_v1::{PalwGenClassV1, PalwGenOffersV1, PalwGenProfileOffersV1};
use crate::palw_gen_job_v1::{PalwGenBodyV1, PalwGenEmbeddingInputV1, PalwGenJobV1};
use crate::palw_gen_v1::{PALW_T12_GEN_PROFILE_CEILINGS_V1, PalwGenProfileCeilingsV1};
use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};
use crate::palw_state_v2::PalwConsensusObjectV2;

/// Key of [`palw_head_label_map_root_v1`].
pub const PALW_HEAD_LABEL_MAP_DOMAIN_V1: &[u8] = b"misaka-palw/head/label-map/v1";
/// Key of [`palw_task_heads_head_set_id_v1`].
pub const PALW_HEAD_SET_DOMAIN_V1: &[u8] = b"misaka-palw/head/head-set/v1";
/// The decode rule's version ([`palw_head_decode_v1`]): part of the fence's `head_set_id`.
pub const PALW_HEAD_DECODE_VERSION_V1: u16 = 1;

/// The tasks (`PalwGenHeadOffersV1::task`, `PalwGenHeadBodyV1::task`).
pub const PALW_HEAD_TASK_SEQUENCE_V1: u8 = 1;
/// A sequence classifier over a pair `a ‖ separator ‖ b`: a cross-encoder reranker, NLI (zero-shot when the entailment label is named).
pub const PALW_HEAD_TASK_PAIR_V1: u8 = 2;
pub const PALW_HEAD_TASK_TOKEN_V1: u8 = 3;
pub const PALW_HEAD_TASK_SPAN_QA_V1: u8 = 4;
pub const PALW_HEAD_TASK_MASKED_LM_V1: u8 = 5;
pub const PALW_HEAD_TASK_IMAGE_V1: u8 = 6;
pub const PALW_HEAD_TASK_DETECTION_V1: u8 = 7;
pub const PALW_HEAD_TASK_SEGMENTATION_V1: u8 = 8;
/// Every task tag this build defines, ascending.
pub const PALW_HEAD_TASKS_V1: [u8; 8] = [1, 2, 3, 4, 5, 6, 7, 8];

/// The decision rule over a row of logits (`PalwGenHeadOffersV1::problem`).
pub const PALW_HEAD_PROBLEM_SINGLE_LABEL_V1: u8 = 1;
pub const PALW_HEAD_PROBLEM_MULTI_LABEL_V1: u8 = 2;
pub const PALW_HEAD_PROBLEM_REGRESSION_V1: u8 = 3;

fn keyed64(key: &[u8], parts: &[&[u8]]) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(key).to_state();
    for part in parts {
        state.update(part);
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// **What a `Head` class offers beyond the generic offers** (Borsh variant 3 of `PalwGenProfileOffersV1`, appended).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwGenHeadOffersV1 {
    /// `PALW_HEAD_TASK_*`.
    pub task: u8,
    /// `PALW_HEAD_PROBLEM_*`.
    pub problem: u8,
    /// The output's last extent: `id2label`'s length; 2 for a span QA head (start, end); the vocabulary for a masked-LM head; the
    /// labels plus one ("no object") plus four box values for a detection head.
    pub labels: u32,
    /// [`palw_head_label_map_root_v1`] over the label strings in index order; the zero hash for a span QA or masked-LM head (the
    /// tokenizer is a masked-LM head's map).
    pub label_map_root: Hash64,
    /// A pair task's ids between its two texts (`[SEP]`, `</s></s>`); empty otherwise.
    pub pair_separator: Vec<u32>,
    /// A pair class as zero-shot NLI: the label index that means "entailment".
    pub entailment_label: Option<u32>,
    /// A masked-LM class: the job scalar that carries the masked row's index (the template's prefix length plus the job's position).
    pub position_scalar: Option<u8>,
}

/// **A `Head` job's body** (Borsh variant 2 of `PalwGenBodyV1`, appended).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwGenHeadBodyV1 {
    /// Text ids under the class tokenizer (a pair is ONE list, `a ‖ separator ‖ b`, formed by the gateway), or one canonical image.
    pub input: PalwGenEmbeddingInputV1,
    /// MUST equal the class's task.
    pub task: u8,
    /// A masked-LM job: the masked id's index in the user's ids (`< tokens`); 0 for every other task.
    pub position: u32,
    /// `EmbeddingI32 = 3`.
    pub output: u8,
}

/// **The label map's root**: `H64(key "misaka-palw/head/label-map/v1", le32(n) ‖ (le32(len) ‖ utf8 label)*)`, labels in index order.
pub fn palw_head_label_map_root_v1(labels: &[&str]) -> Hash64 {
    let mut parts: Vec<Vec<u8>> = vec![(labels.len() as u32).to_le_bytes().to_vec()];
    for l in labels {
        let mut p = (l.len() as u32).to_le_bytes().to_vec();
        p.extend_from_slice(l.as_bytes());
        parts.push(p);
    }
    keyed64(PALW_HEAD_LABEL_MAP_DOMAIN_V1, &parts.iter().map(Vec::as_slice).collect::<Vec<_>>())
}

/// **The head set's identity**: the task tags, the problem tags and the decode rule's version this build implements. The fence names
/// it; a ruleset that names another is refused.
pub fn palw_task_heads_head_set_id_v1() -> Hash64 {
    keyed64(
        PALW_HEAD_SET_DOMAIN_V1,
        &[
            &PALW_HEAD_TASKS_V1,
            &[PALW_HEAD_PROBLEM_SINGLE_LABEL_V1, PALW_HEAD_PROBLEM_MULTI_LABEL_V1, PALW_HEAD_PROBLEM_REGRESSION_V1],
            &PALW_HEAD_DECODE_VERSION_V1.to_le_bytes(),
        ],
    )
}

/// **One decision over a verified output** (`HEAD_DECODE_V1`), never consensus: a pure function of the output row and the offers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwHeadDecisionV1 {
    /// Single-label: the arg-max (the smallest index among the maxima).
    Label(u32),
    /// Multi-label: every label whose logit is `> 0` (sigmoid `> ½`, exactly, in integers), ascending.
    Labels(Vec<u32>),
    /// Regression: the raw value, in the class's unit.
    Value(i64),
}

/// **`HEAD_DECODE_V1` over one row** of `[rows, labels]`: the decision the offers' `problem` names. `None` for an empty row.
pub fn palw_head_decode_v1(problem: u8, row: &[i32]) -> Option<PalwHeadDecisionV1> {
    if row.is_empty() {
        return None;
    }
    Some(match problem {
        PALW_HEAD_PROBLEM_MULTI_LABEL_V1 => {
            PalwHeadDecisionV1::Labels(row.iter().enumerate().filter(|(_, v)| **v > 0).map(|(i, _)| i as u32).collect())
        }
        PALW_HEAD_PROBLEM_REGRESSION_V1 => PalwHeadDecisionV1::Value(row[0] as i64),
        _ => PalwHeadDecisionV1::Label(palw_head_argmax_v1(row)),
    })
}

/// The smallest index among the maxima of a non-empty row.
pub fn palw_head_argmax_v1(row: &[i32]) -> u32 {
    let mut best = 0usize;
    for (i, v) in row.iter().enumerate() {
        if *v > row[best] {
            best = i;
        }
    }
    best as u32
}

/// **The best extractive span** of a span-QA output (`HEAD_DECODE_V1`): over rows `context..count`, the `(s, e)` with `s ≤ e < s +
/// max_len` maximising `start[s] + end[e]` (in `i64`), ties to the smallest `(s, e)`. `None` when the context is empty.
pub fn palw_head_best_span_v1(start: &[i32], end: &[i32], context: usize, count: usize, max_len: usize) -> Option<(usize, usize)> {
    let count = count.min(start.len()).min(end.len());
    let mut best: Option<((usize, usize), i64)> = None;
    for s in context..count {
        for e in s..count.min(s.saturating_add(max_len.max(1))) {
            let v = start[s] as i64 + end[e] as i64;
            if best.is_none_or(|(_, b)| v > b) {
                best = Some(((s, e), v));
            }
        }
    }
    best.map(|(p, _)| p)
}

/// **`Params::palw_task_heads_v1`'s value**: the activation, the head set this build implements, and the `Head` profile's own ceilings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwTaskHeadsFenceV1 {
    pub activation: ForkActivation,
    pub head_set_id: Hash64,
    pub ceilings: PalwGenProfileCeilingsV1,
}

impl PalwTaskHeadsFenceV1 {
    /// The fence as THIS build states it.
    pub fn this_build_v1(activation: ForkActivation, ceilings: PalwGenProfileCeilingsV1) -> Self {
        Self { activation, head_set_id: palw_task_heads_head_set_id_v1(), ceilings }
    }

    /// testnet-12's value at a height (proposed): the Embedding profile's provisional ceilings — a head is an encoder's pipeline.
    pub fn testnet12_v1(activation: ForkActivation) -> Self {
        Self::this_build_v1(activation, PALW_T12_GEN_PROFILE_CEILINGS_V1)
    }

    /// What the fence adds to a fingerprint beside its height (`consensus_params_id` and `consensus_schedule_id` write these bytes).
    pub(crate) fn write_value_into(&self, h: &mut kaspa_hashes::ConsensusParamsId) {
        h.write(self.head_set_id.as_byte_slice());
        self.ceilings.write_into(h);
    }
}

/// **The entry that arms the fence** (a drill's; no flag-day list carries it): testnet-12's value and the bundle's mirror.
pub const PALW_DRILL_TASK_HEADS_ENTRY: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_task_heads_v1",
    set: |params, at| {
        params.palw_task_heads_v1 = at.map(PalwTaskHeadsFenceV1::testnet12_v1);
        params.sync_palw_task_heads_v1();
    },
};

impl Params {
    /// `palw_task_heads_v1`, resolved: `Some` only on a `ConsensusV2` network that armed it (and not `never()`).
    pub fn palw_task_heads_v1_fence(&self) -> Option<PalwTaskHeadsFenceV1> {
        match (&self.palw_consensus_mode, self.palw_task_heads_v1) {
            (PalwConsensusMode::ConsensusV2(_), Some(fence)) if fence.activation != ForkActivation::never() => Some(fence),
            _ => None,
        }
    }

    /// Whether the `Head` profile is in force at `daa_score`: `false` on every shipped preset.
    pub fn palw_task_heads_active_at(&self, daa_score: u64) -> bool {
        self.palw_task_heads_v1_fence().is_some_and(|f| f.activation.is_active(daa_score))
    }

    /// **Mirror the fence onto the V2 bundle** (the fold and the acceptance path read the bundle): its height and the `Head` profile's
    /// cap on claims in flight. Call it wherever the fence is set on an assembled ruleset.
    pub fn sync_palw_task_heads_v1(&mut self) {
        let fence = self.palw_task_heads_v1.filter(|f| f.activation != ForkActivation::never());
        let from_daa = fence.map(|f| f.activation.daa_score());
        let cap = fence.map_or(0, |f| f.ceilings.max_inflight_claims);
        if let PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_task_heads_from_daa(from_daa).with_task_heads_max_inflight_claims(cap);
        }
    }

    /// **The fence's refusal**, asked by [`Params::validate_palw_v2`]: the V2 bundle's mirror equal to the fence, and **no armed height**
    /// — the fence is dormant in this build (the common rule for every consensus change before the full-activation release). A
    /// `Some(never())` value is absence and validates.
    pub fn validate_palw_task_heads_v1(&self) -> Result<(), PalwModeV2Error> {
        self.palw_task_heads_v1_mirror_agrees()?;
        if self.palw_task_heads_v1.is_some_and(|f| f.activation != ForkActivation::never()) {
            return Err(PalwModeV2Error::Invalid(
                "palw_task_heads_v1 cannot be armed by this build: the Head profile is dormant until the full-activation release",
            ));
        }
        Ok(())
    }

    /// The V2 bundle's mirror of the fence (its height and the `Head` cap on claims in flight) equals the fence.
    fn palw_task_heads_v1_mirror_agrees(&self) -> Result<(), PalwModeV2Error> {
        let mirror = match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(bundle) => {
                (bundle.state.task_heads_from_daa(), bundle.state.task_heads_max_inflight_claims())
            }
            _ => (None, 0),
        };
        let fence = self.palw_task_heads_v1.filter(|f| f.activation != ForkActivation::never());
        let armed = (fence.map(|f| f.activation.daa_score()), fence.map_or(0, |f| f.ceilings.max_inflight_claims));
        if mirror != armed {
            return Err(PalwModeV2Error::Invalid(
                "palw_task_heads_v1 disagrees with the V2 bundle's mirror: mirror it with Params::sync_palw_task_heads_v1",
            ));
        }
        Ok(())
    }

    /// **What arming the fence needs beyond this build's refusal** — the conditions the full-activation release checks: the mirror
    /// equal to the fence; this build's head set; the ceilings inside the format's caps; a ConsensusV2 network with `palw_gen_v1` in
    /// force at or below it. `Ok` for an unarmed fence.
    pub fn palw_task_heads_v1_arming_preconditions(&self) -> Result<(), PalwModeV2Error> {
        self.palw_task_heads_v1_mirror_agrees()?;
        let Some(fence) = self.palw_task_heads_v1.filter(|f| f.activation != ForkActivation::never()) else { return Ok(()) };
        if !matches!(self.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)) {
            return Err(PalwModeV2Error::Invalid("palw_task_heads_v1 is a ConsensusV2 rule: this ruleset has no V2 bundle"));
        }
        if fence.head_set_id != palw_task_heads_head_set_id_v1() {
            return Err(PalwModeV2Error::Invalid("palw_task_heads_v1 names a head set this build does not implement (head_set_id)"));
        }
        fence.ceilings.within_format_caps().map_err(PalwModeV2Error::Invalid)?;
        let gen_ok = self
            .palw_gen_v1
            .is_some_and(|g| g.activation != ForkActivation::never() && g.activation.daa_score() <= fence.activation.daa_score());
        if !gen_ok {
            return Err(PalwModeV2Error::Invalid(
                "palw_task_heads_v1 needs palw_gen_v1 in force at or below it: a Head class is a generative class",
            ));
        }
        Ok(())
    }
}

// ---- the A-2 predicate --------------------------------------------------------------------------------------------------------------

/// A job body int-12 cannot decode: the appended `Head` variant.
pub fn palw_gen_body_is_head_v1(body: &PalwGenBodyV1) -> bool {
    matches!(body, PalwGenBodyV1::Head(_))
}

/// A job int-12 cannot decode.
pub fn palw_gen_job_needs_task_heads_v1(job: &PalwGenJobV1) -> bool {
    palw_gen_body_is_head_v1(&job.body)
}

/// Offers int-12 cannot decode: the appended `Head` variant.
pub fn palw_gen_offers_need_task_heads_v1(offers: &PalwGenOffersV1) -> bool {
    matches!(offers.profile, PalwGenProfileOffersV1::Head(_))
}

/// A class int-12 cannot decode (its profile BYTE alone is not: a `u8` decodes whatever its value).
pub fn palw_gen_class_needs_task_heads_v1(class: &PalwGenClassV1) -> bool {
    palw_gen_offers_need_task_heads_v1(&class.offers)
}

fn binding_needs(b: &crate::palw_gen_close_v1::PalwGenBindingV1) -> bool {
    matches!(b, crate::palw_gen_close_v1::PalwGenBindingV1::Tensor(t) if palw_gen_job_needs_task_heads_v1(&t.job))
}

/// A court proof int-12 cannot decode: a generative close whose tensor binding carries a `Head` job.
pub fn palw_court_proof_needs_task_heads_v1(proof: &crate::palw_court_v2::PalwCourtVerdictProofV2) -> bool {
    use crate::palw_court_v2::PalwCourtVerdictProofV2 as P;
    match proof {
        P::GenCone { close } | P::GenDissection { bottom: close } => binding_needs(&close.binding),
        P::GenOutputTile { close } => palw_gen_job_needs_task_heads_v1(&close.binding.job),
        _ => false,
    }
}

/// **The one A-2 predicate**: does this object carry a `Head` variant (an offers variant 3 or a body variant 2) that the int-12 build
/// cannot decode? Below `palw_task_heads_v1` such an object is read as int-12 reads its bytes — not decodable (see the module doc).
pub fn palw_object_needs_task_heads_v1(object: &PalwConsensusObjectV2) -> bool {
    match object {
        PalwConsensusObjectV2::ClassRegisteredGenV1 { admission, .. } => palw_gen_class_needs_task_heads_v1(&admission.class),
        PalwConsensusObjectV2::GenTensorCommitted { job, .. } => palw_gen_job_needs_task_heads_v1(job),
        PalwConsensusObjectV2::CourtClosed { proof, .. } => palw_court_proof_needs_task_heads_v1(proof),
        PalwConsensusObjectV2::CourtGenRootClaimed { root, .. } => binding_needs(&root.finalize.binding),
        PalwConsensusObjectV2::GenShardCourtAccused { accusation } => palw_court_proof_needs_task_heads_v1(&accusation.proof),
        PalwConsensusObjectV2::TirShardCourtAccused { accusation } => palw_court_proof_needs_task_heads_v1(&accusation.proof),
        // A signed-expiry envelope (tag 108) is judged as the registration it wraps (the acceptance walk unwraps it first; the
        // isolation gate reads the whole carrier, which int-12 cannot decode either way).
        PalwConsensusObjectV2::SignedRegistrationV1 { registration, .. } => palw_object_needs_task_heads_v1(registration),
        _ => false,
    }
}

/// **The header-context half of the tensor door for a `Head` job**: at the containing block's height, why a version-10 free-prompt
/// payload whose job body is `Head` is refused — below `palw_task_heads_v1` (on a ruleset that does not schedule it, at every height)
/// — or `None`. int-12's isolation door refuses the same payload as undecodable, so the block is invalid on both builds.
pub fn palw_fp_head_job_refusal_at_v1(payload: &[u8], heads_active: bool) -> Option<&'static str> {
    if heads_active || !crate::palw_gen_claim_v1::palw_fp_payload_is_gen_v1(payload) {
        return None;
    }
    let decoded: crate::palw_freeprompt_v3::PalwFpCommitmentTxPayloadV3 = borsh::from_slice(payload).ok()?;
    match &decoded.commitment.job.tail {
        Some(crate::palw_freeprompt_v3::PalwFpJobTailV1::Gen(tail)) if palw_gen_body_is_head_v1(&tail.body) => {
            Some("a Head-profile tensor job below palw_task_heads_v1 (a body the int-12 build cannot decode)")
        }
        _ => None,
    }
}
