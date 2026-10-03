//! **RFC-0001 §2.6 stage 2: FP job version 11 — the prefix-state receipt; dormant behind `palw_fp_prefix_state`.**
//!
//! (The module's logic follows its fence: this first section is the fence's own plumbing — the accessor, the
//! prerequisites `validate_palw_v2` asks by name, and the drill entry a salted chain arms it with.)

use crate::config::params::{ForkActivation, PalwPostLaunchFenceV1, Params};
use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};

/// **The entry a drill arms `palw_fp_prefix_state` with** (`--palw-drill-fp-prefix-state-at`,
/// [`crate::config::drill`]). In NO testnet-12 flag-day list: dormant on every network.
pub const PALW_DRILL_FP_PREFIX_STATE_ENTRY: PalwPostLaunchFenceV1 =
    PalwPostLaunchFenceV1 { name: "palw_fp_prefix_state", set: |params, at| params.palw_fp_prefix_state = at };

/// The drill's one-entry list.
pub const PALW_DRILL_FP_PREFIX_STATE_FENCES_V1: &[PalwPostLaunchFenceV1] = &[PALW_DRILL_FP_PREFIX_STATE_ENTRY];

impl Params {
    /// `palw_fp_prefix_state`, resolved: `Some` only on a `ConsensusV2` network that armed it.
    pub fn palw_fp_prefix_state_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_fp_prefix_state) {
            (PalwConsensusMode::ConsensusV2(_), Some(fence)) => Some(fence),
            _ => None,
        }
    }

    pub fn palw_fp_prefix_state_active_at(&self, daa_score: u64) -> bool {
        self.palw_fp_prefix_state_fence().is_some_and(|f| f.is_active(daa_score))
    }

    /// **`palw_fp_prefix_state`'s own refusals**, asked by [`Params::validate_palw_v2`]: a `ConsensusV2` rule over its prerequisites,
    /// each in force at or below it. A `Some(never())` value is dormant and passes.
    pub fn validate_palw_fp_prefix_state_v1(&self) -> Result<(), PalwModeV2Error> {
        let Some(fence) = self.palw_fp_prefix_state.filter(|f| *f != ForkActivation::never()) else { return Ok(()) };
        if !matches!(self.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)) {
            return Err(PalwModeV2Error::Invalid("palw_fp_prefix_state is armed on a network that is not ConsensusV2"));
        }
        let at_or_below =
            |other: Option<ForkActivation>| other.is_some_and(|o| o != ForkActivation::never() && o.daa_score() <= fence.daa_score());
        if !at_or_below(self.palw_fp_derived_work) {
            return Err(PalwModeV2Error::Invalid(
                "palw_fp_prefix_state needs palw_fp_derived_work in force at or below it: the price reads the state",
            ));
        }
        if !at_or_below(self.palw_fp_decode_rules) {
            return Err(PalwModeV2Error::Invalid(
                "palw_fp_prefix_state needs palw_fp_decode_rules in force at or below it: V11 embeds FP Job V4",
            ));
        }
        Ok(())
    }
}

/// **The entry a drill arms `palw_fp_prefix_inherit` with** (`--palw-drill-fp-prefix-inherit-at`). In NO testnet-12 flag-day
/// list: dormant on every network.
pub const PALW_DRILL_FP_PREFIX_INHERIT_ENTRY: PalwPostLaunchFenceV1 =
    PalwPostLaunchFenceV1 { name: "palw_fp_prefix_inherit", set: |params, at| params.palw_fp_prefix_inherit = at };

/// The drill's one-entry list.
pub const PALW_DRILL_FP_PREFIX_INHERIT_FENCES_V1: &[PalwPostLaunchFenceV1] = &[PALW_DRILL_FP_PREFIX_INHERIT_ENTRY];

impl Params {
    /// `palw_fp_prefix_inherit`, resolved: `Some` only on a `ConsensusV2` network that armed it.
    pub fn palw_fp_prefix_inherit_fence(&self) -> Option<ForkActivation> {
        match (&self.palw_consensus_mode, self.palw_fp_prefix_inherit) {
            (PalwConsensusMode::ConsensusV2(_), Some(fence)) => Some(fence),
            _ => None,
        }
    }

    pub fn palw_fp_prefix_inherit_active_at(&self, daa_score: u64) -> bool {
        self.palw_fp_prefix_inherit_fence().is_some_and(|f| f.is_active(daa_score))
    }

    /// **`palw_fp_prefix_inherit`'s own refusals**: a `ConsensusV2` rule over `palw_fp_prefix_state` in force at or below it.
    pub fn validate_palw_fp_prefix_inherit_v1(&self) -> Result<(), PalwModeV2Error> {
        let Some(fence) = self.palw_fp_prefix_inherit.filter(|f| *f != ForkActivation::never()) else { return Ok(()) };
        if !matches!(self.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)) {
            return Err(PalwModeV2Error::Invalid("palw_fp_prefix_inherit is armed on a network that is not ConsensusV2"));
        }
        let ok = self
            .palw_fp_prefix_state
            .is_some_and(|o| o != ForkActivation::never() && o.daa_score() <= fence.daa_score());
        if !ok {
            return Err(PalwModeV2Error::Invalid(
                "palw_fp_prefix_inherit needs palw_fp_prefix_state in force at or below it: version 12 is a prefix-state job",
            ));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------
// FP job version 11: the prefix-state receipt
// ---------------------------------------------------------------------------------------------
//
// **What it is.** ADR-0145 §6 priced a prefix the chain had not paid for as `KvReused` and credited the new positions
// only — and left the object that names the state to "a later payload version". This is it: an FP job whose version
// word says 11, which is a V4 job (every V3 field, then its `DecodeConfigV4`) followed by the [`PalwFpPrefixStateV1`]
// it consumed. The state is INSIDE the job, so inside the claim id the executor signs: nobody relaying the payload can
// change it, and the pay the chain derives from it ([`crate::palw_freeprompt_v3::fp_derive_work_from_state_v1`]) is the
// signer's own statement.
//
// **What the chain does with it, and what it does not.** The extraction walk carries the state to the fold
// (`FreePromptCommitted.consumed_prefix_state`), where the price reads it: a non-genesis state of the class credits the
// leaves past its prefix and no others. Nothing the executor writes can raise its pay — declaring a state only ever
// LOWERS the credit, so the lie worth telling is silence, and silence is genesis (the V4 job). The chain does not verify
// that `state_root` is the true state of the prompt's first `prefix_tokens` ids: that is a fact about a deterministic
// integer computation any seat can redo, and a root nobody can be paid more for is not an attack surface. A seat that
// holds the prefix (its own prefix cache, or a recomputation) checks the root
// ([`palw_fp_prefix_state_root_v1`]) and files what it finds.
//
// **What it does not do yet (stage 2b).** The committed fold still covers every prefill position, so a producer that
// holds the prefix's KV state but not its tiles still walks the prefix to commit it. Removing that walk needs the
// prefix's leaves to be INHERITED (a leaf hash that is a function of the state root and the leaf's index, not of a
// tile) and every court that hashes a leaf to refuse an inherited one; the design is in docs/spec/17 under "inherited
// leaves". Until then this receipt is what lets the chain price an unpaid cache honestly, and the node-side prefix cache
// (RFC-0001 §2.6 stage 1) serves the answers that need no claim.

use crate::Hash64;
use crate::palw_freeprompt_v3::{
    PalwFpCommitmentTxPayloadV3, PalwFpDecodeRulesV1, PalwFpJobTailV1, PalwFpPrefixStateV1, PalwFpV3Error, PalwFreePromptJobV3,
};
use crate::palw_prompt_ids_v1::PalwPromptIdsFormV1;

/// **The prefix-state job's version**: 11 (7 is V4, 8 V5, 9 an evaluation job, 10 a tensor job).
pub const PALW_FP_PREFIX_VERSION: u16 = 11;
/// **The inherited-prefix job's version**: 12 — a prefix-state job (version 11's shape, its tail the same
/// [`PalwFpPrefixStateV1`]) whose first `k` prefill positions' step leaves are bound to a job-independent prefix context
/// (stage 2b, `palw_fp_prefix_inherit`; [`PalwJobContextV2::inherited_prefix_v1`]).
pub const PALW_FP_PREFIX_INHERIT_VERSION: u16 = 12;
/// The inherited-prefix job id's key.
pub const PALW_FP_PREFIX_INHERIT_DOMAIN_JOB_ID: &[u8] = b"misaka-palw/fp-prefix-inherit/job-id/v1";
/// The prefix-state job id's key: a domain no other job's id uses.
pub const PALW_FP_PREFIX_DOMAIN_JOB_ID: &[u8] = b"misaka-palw/fp-prefix/job-id/v1";
/// The domain of a prefix state's root ([`palw_fp_prefix_state_root_v1`]).
pub const PALW_FP_PREFIX_DOMAIN_STATE_ROOT: &[u8] = b"misaka-palw/fp-prefix/state-root/v1";

/// Why a job is not a well-formed prefix-state job.
#[derive(thiserror::Error, Clone, Debug, PartialEq, Eq)]
pub enum PalwFpPrefixErrorV1 {
    #[error("palw_fp_prefix_state is not in force")]
    NotArmed,
    #[error("job version {version} is not the prefix-state job (version 11)")]
    NotAPrefixJob { version: u16 },
    #[error("a prefix-state job carries its V4 decode rules")]
    NoDecode,
    #[error("a prefix-state job carries a prefix state in its tail")]
    NoTail,
    #[error("the prefix state is genesis (no positions, or the empty root): that is a V4 job, and one encoding names one behaviour")]
    StateIsGenesis,
    #[error("the prefix state names another class than the job's")]
    StateClassMismatch,
    #[error("the prefix covers {prefix} positions of a {prompt}-token prompt: at least one position must be new")]
    PrefixNotBelowPrompt { prefix: u32, prompt: u32 },
    #[error("a V4 rule refused the job: {0}")]
    V4(PalwFpV3Error),
}

/// **`fp_job_id_prefix_v1`**: `H64(key "misaka-palw/fp-prefix/job-id/v1", le64(|bytes|) ‖ bytes)` over the job's whole
/// borsh (every V3 field, its decode rules, its prefix state) — the V3/V4/V5 ids' construction under this version's key.
pub fn fp_job_id_prefix_v1(job: &PalwFreePromptJobV3) -> Hash64 {
    let bytes = borsh::to_vec(job).expect("a free-prompt job is borsh-serializable");
    let key = if job.is_prefix_inherit() { PALW_FP_PREFIX_INHERIT_DOMAIN_JOB_ID } else { PALW_FP_PREFIX_DOMAIN_JOB_ID };
    let mut state = blake2b_simd::Params::new().hash_length(64).key(key).to_state();
    state.update(&(bytes.len() as u64).to_le_bytes());
    state.update(&bytes);
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// **A prefix state's root**: `H64(key "misaka-palw/fp-prefix/state-root/v1", class_id ‖ le32(prefix_tokens) ‖
/// kv_digest)`, where `kv_digest` is the family's digest of the K/V state after the prefix's last position (the dense A16
/// tier's: BLAKE2b-512 over every layer's keys then values as little-endian `i32`s, in layer order). Anyone holding the
/// prefix's state — a seat's cache, a recomputation — derives the same root; the class and the length are inside it, so a
/// state of another class or another prefix is another root.
pub fn palw_fp_prefix_state_root_v1(class_id: &Hash64, prefix_tokens: u32, kv_digest: &Hash64) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_FP_PREFIX_DOMAIN_STATE_ROOT).to_state();
    state.update(class_id.as_byte_slice());
    state.update(&prefix_tokens.to_le_bytes());
    state.update(kv_digest.as_byte_slice());
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// The prefix state a prefix-state job carries — `None` for any other job.
pub fn palw_fp_prefix_tail_v1(job: &PalwFreePromptJobV3) -> Option<&PalwFpPrefixStateV1> {
    match (&job.tail, job.is_prefix_state()) {
        (Some(PalwFpJobTailV1::Prefix(state)), true) => Some(state),
        _ => None,
    }
}

/// **A prefix-state job's own shape rules** (everything the V4 rules do not already say): version 11 with its decode
/// rules and its tail; the state non-genesis, of the job's class, covering at least one position and at least one fewer
/// than the prompt has. The V4 rules run on its V4 view ([`palw_fp_prefix_stand_in_v1`]) in the callers.
pub fn palw_fp_prefix_shape_v1(job: &PalwFreePromptJobV3) -> Result<PalwFpPrefixStateV1, PalwFpPrefixErrorV1> {
    if !job.is_prefix_state() {
        return Err(PalwFpPrefixErrorV1::NotAPrefixJob { version: job.version });
    }
    if job.decode.is_none() {
        return Err(PalwFpPrefixErrorV1::NoDecode);
    }
    let Some(state) = palw_fp_prefix_tail_v1(job).copied() else { return Err(PalwFpPrefixErrorV1::NoTail) };
    if state.is_genesis() {
        return Err(PalwFpPrefixErrorV1::StateIsGenesis);
    }
    if state.class_id != job.class_id {
        return Err(PalwFpPrefixErrorV1::StateClassMismatch);
    }
    if state.prefix_tokens >= job.prompt_tokens {
        return Err(PalwFpPrefixErrorV1::PrefixNotBelowPrompt { prefix: state.prefix_tokens, prompt: job.prompt_tokens });
    }
    Ok(state)
}

/// **The prefix-state claim's V4 stand-in**: the same payload at FP Job V4 (version 7, no tail), so every rule the lane
/// applies to a V4 commitment — the network, the signer's shape, the prompt ids against their hash and form, the context,
/// the executed count and stop, the ladder, the ruleset's caps — applies unchanged, and only the version rule is this
/// module's. The signature is NOT checked on it: the claim id is the real commitment's.
pub fn palw_fp_prefix_stand_in_v1(payload: &PalwFpCommitmentTxPayloadV3) -> PalwFpCommitmentTxPayloadV3 {
    let mut stand_in = payload.clone();
    stand_in.commitment.job.version = crate::palw_freeprompt_v3::PALW_FP_V4_VERSION;
    stand_in.commitment.job.tail = None;
    stand_in
}

/// The walk's refusal text for a version-12 claim over a class that is not inheritance-safe.
pub const PALW_FP_PREFIX_INHERIT_CLASS_REFUSAL_V1: &str =
    "an inherited-prefix claim (FP job version 12) over a class whose leaves are not inheritance-safe (fused attention, KV aux, held, IR/generative, or no published integer-lane profile)";

/// **RFC-0001 stage 2b admission: which classes may carry inherited prefix leaves.** Only a class proven so: a dense integer-lane class
/// (`PalwStepLaneV1::Int32`) with a published profile, no KV aux series (`kv_chunk_calls == 0`), no fused-attention court window,
/// not held, not an IR or generative class. Everything else (fused attention, KV aux, checkpoint-inheriting kinds, unproven kinds)
/// is refused by name. `Err` carries the reason.
pub fn palw_fp_prefix_inherit_class_safe_v1(
    profile: Option<&crate::palw_step::PalwShapeProfileV3>,
    held: bool,
    ir_or_generative: bool,
    has_court_window: bool,
) -> Result<(), &'static str> {
    if held {
        return Err("a held class's leaves are not inheritance-safe");
    }
    if ir_or_generative {
        return Err("an IR or generative class is not a proven inheritance-safe kind");
    }
    if has_court_window {
        return Err("a class with a fused-attention court window is not inheritance-safe");
    }
    let Some(profile) = profile else { return Err("the class published no shape profile") };
    if profile.lane != crate::palw_step::PalwStepLaneV1::Int32 {
        return Err("only the integer lane is a proven inheritance-safe kind");
    }
    if profile.kv_chunk_calls != 0 {
        return Err("a class with a KV aux series is not inheritance-safe");
    }
    Ok(())
}

/// **Is this FP payload a prefix-state claim's?** Its job's version word — bytes 2..4, after the payload's own version —
/// is [`PALW_FP_PREFIX_VERSION`]. Nothing else is read.
pub fn palw_fp_payload_is_prefix_v1(payload: &[u8]) -> bool {
    payload.get(2..4) == Some(&PALW_FP_PREFIX_VERSION.to_le_bytes()[..]) || palw_fp_payload_is_prefix_inherit_v1(payload)
}

/// **Is this FP payload an inherited-prefix claim's** (version 12)?
pub fn palw_fp_payload_is_prefix_inherit_v1(payload: &[u8]) -> bool {
    payload.get(2..4) == Some(&PALW_FP_PREFIX_INHERIT_VERSION.to_le_bytes()[..])
}

fn prefix_refused(e: PalwFpPrefixErrorV1) -> PalwFpV3Error {
    PalwFpV3Error::PrefixStateClaim(e.to_string())
}

/// **The isolation door for a prefix-state claim**, height-free as isolation is: the payload decodes with its tail; its V4
/// stand-in passes the lane's shape rules under the same door arguments an FP commitment meets; and the job's own shape
/// holds ([`palw_fp_prefix_shape_v1`]). Only where the ruleset carries `Params::palw_fp_prefix_state`; the header-context
/// door decides the height ([`palw_fp_prefix_refusal_at_v1`]).
pub fn validate_palw_fp_prefix_commitment_tx_v1(
    payload: &[u8],
    panel_da_admissible: bool,
    prompt_ids_form: PalwPromptIdsFormV1,
    work_leaves_cap: u64,
    decode_rules: PalwFpDecodeRulesV1,
) -> Result<(), PalwFpV3Error> {
    let payload: PalwFpCommitmentTxPayloadV3 =
        borsh::from_slice(payload).map_err(|_| PalwFpV3Error::PrefixStateClaim("the payload does not decode".to_string()))?;
    palw_fp_prefix_shape_v1(&payload.commitment.job).map_err(prefix_refused)?;
    palw_fp_prefix_stand_in_v1(&payload).validate_shape_under_ruleset_v4(
        panel_da_admissible,
        work_leaves_cap,
        None,
        prompt_ids_form,
        decode_rules,
    )
}

/// **The free-prompt door with prefix-state claims**: a version-11 payload goes to
/// [`validate_palw_fp_prefix_commitment_tx_v1`] where `prefix_door` (the ruleset carries `palw_fp_prefix_state`), and every
/// other payload — a prefix-state claim's too, where the door is shut — to the evaluation / tensor-aware door under it,
/// which refuses version 11 by name (`UnsupportedVersion`), as a build without the fence does. So a build that schedules
/// the fence and one that does not agree on every transaction below it.
#[allow(clippy::too_many_arguments)]
pub fn validate_palw_fp_commitment_tx_under_v8(
    payload: &[u8],
    panel_da_admissible: bool,
    prompt_ids_form: PalwPromptIdsFormV1,
    work_leaves_cap: u64,
    decode_rules: PalwFpDecodeRulesV1,
    improvement_door: bool,
    gen_door: bool,
    prefix_door: bool,
    inherit_door: bool,
) -> Result<(), PalwFpV3Error> {
    // A version-12 payload needs the inherited-prefix door as well (stage 2b); shut, it falls through and is refused by name.
    if prefix_door && palw_fp_payload_is_prefix_v1(payload) && (inherit_door || !palw_fp_payload_is_prefix_inherit_v1(payload)) {
        return validate_palw_fp_prefix_commitment_tx_v1(payload, panel_da_admissible, prompt_ids_form, work_leaves_cap, decode_rules);
    }
    crate::palw_gen_claim_v1::validate_palw_fp_commitment_tx_under_v7(
        payload,
        panel_da_admissible,
        prompt_ids_form,
        work_leaves_cap,
        decode_rules,
        improvement_door,
        gen_door,
    )
}

/// **Why the containing block's height refuses a prefix-state claim** — the header-context half of the door.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwFpPrefixHeightRefusalV1 {
    /// Below `Params::palw_fp_prefix_state`: no prefix-state claim exists yet.
    BelowPrefixState,
    /// Below `Params::palw_fp_decode_rules`, whose V4 rules the job carries.
    BelowDecodeRules,
    /// A version-12 claim below `Params::palw_fp_prefix_inherit`.
    BelowInherit,
}

impl PalwFpPrefixHeightRefusalV1 {
    /// The refusal's name, as the transaction rule error carries it.
    pub fn why(self) -> &'static str {
        match self {
            Self::BelowPrefixState => "a prefix-state claim (FP job version 11) below Params::palw_fp_prefix_state",
            Self::BelowDecodeRules => "a prefix-state claim below Params::palw_fp_decode_rules, whose V4 rules its job carries",
            Self::BelowInherit => "an inherited-prefix claim (FP job version 12) below Params::palw_fp_prefix_inherit",
        }
    }
}

/// **The header-context half of the prefix-state door**: at the containing block's height, why a prefix-state claim is
/// refused. `None` for any other payload and for one the height admits.
pub fn palw_fp_prefix_refusal_at_v1(
    payload: &[u8],
    prefix_active: bool,
    decode_rules_active: bool,
    inherit_active: bool,
) -> Option<PalwFpPrefixHeightRefusalV1> {
    if !palw_fp_payload_is_prefix_v1(payload) {
        return None;
    }
    if !prefix_active {
        return Some(PalwFpPrefixHeightRefusalV1::BelowPrefixState);
    }
    if !decode_rules_active {
        return Some(PalwFpPrefixHeightRefusalV1::BelowDecodeRules);
    }
    if palw_fp_payload_is_prefix_inherit_v1(payload) && !inherit_active {
        return Some(PalwFpPrefixHeightRefusalV1::BelowInherit);
    }
    None
}

/// What the extraction walk reads of a payload when `prefix_armed`: for a prefix-state claim, the V4 stand-in the stateless
/// rules run on and the state the fold prices; for any other payload, itself and the state it names (genesis).
/// `Err` is the walk's skip reason.
pub fn palw_fp_prefix_walk_view_v1(
    payload: &PalwFpCommitmentTxPayloadV3,
    prefix_armed: bool,
    inherit_armed: bool,
) -> Result<(PalwFpCommitmentTxPayloadV3, PalwFpPrefixStateV1), &'static str> {
    if !payload.commitment.job.is_prefix_state() {
        return Ok((payload.clone(), payload.consumed_prefix_state_v1()));
    }
    if !prefix_armed {
        return Err("a prefix-state claim (FP job version 11) below palw_fp_prefix_state");
    }
    if payload.commitment.job.is_prefix_inherit() && !inherit_armed {
        return Err("an inherited-prefix claim (FP job version 12) below palw_fp_prefix_inherit");
    }
    match palw_fp_prefix_shape_v1(&payload.commitment.job) {
        Ok(state) => Ok((palw_fp_prefix_stand_in_v1(payload), state)),
        Err(_) => Err("a prefix-state claim whose job is not well formed"),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn inheritance_admission_refuses_every_unproven_kind_by_name() {
        use super::palw_fp_prefix_inherit_class_safe_v1 as safe;
        let base = crate::palw_base0_profile::base0_profile_v1(crate::palw_base0_profile::PALW_RC_BASE0_GEOMETRY).expect("floor profile");
        assert_eq!(safe(Some(&base), false, false, false), Ok(()));
        assert!(safe(Some(&base), true, false, false).unwrap_err().contains("held"));
        assert!(safe(Some(&base), false, true, false).unwrap_err().contains("IR or generative"));
        assert!(safe(Some(&base), false, false, true).unwrap_err().contains("fused-attention"));
        assert!(safe(None, false, false, false).unwrap_err().contains("no shape profile"));
        let mut kv = base.clone();
        kv.kv_chunk_calls = 4;
        assert!(safe(Some(&kv), false, false, false).unwrap_err().contains("KV aux"));
        let mut float = base.clone();
        float.lane = crate::palw_step::PalwStepLaneV1::Float32;
        assert!(safe(Some(&float), false, false, false).unwrap_err().contains("integer lane"));
    }

    use super::*;
    use crate::constants::TX_VERSION;
    use crate::palw_decode_pipeline_v4::DecodeConfigV4;
    use crate::palw_fp_objects_v3::{
        PalwFpClassCapsV1, PalwFpDerivedWorkCapV1, palw_fp_objects_from_accepted_txs_by_class_v1,
    };
    use crate::palw_freeprompt_v3::{
        PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_V3_VERSION, PALW_FP_V4_VERSION, PalwFpStopReasonV3, PalwFreePromptCommitmentV3,
        PalwFreePromptParamsV3, fp_claim_id_v3, fp_job_id_v3, fp_trace_manifest_v3,
    };
    use crate::palw_fp_tokenizer_v1::PalwFpTokenizerRuleV1;
    use crate::palw_state_v2::PalwConsensusObjectV2;
    use crate::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT;
    use crate::tx::{Transaction, TransactionId, TransactionOutpoint};

    fn h64(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    fn net() -> Hash64 {
        h64(0x4E)
    }

    fn freeprompt() -> PalwFreePromptParamsV3 {
        crate::palw_fp_devnet_v3::palw_fp_devnet_bundle_for_tests(h64(1), h64(0xCA7), h64(0xC0757)).unwrap().freeprompt
    }

    const PROMPT: u32 = 12;
    const PREFIX: u32 = 8;

    fn state() -> PalwFpPrefixStateV1 {
        PalwFpPrefixStateV1 { state_root: h64(0x51), prefix_tokens: PREFIX, class_id: h64(1) }
    }

    /// A V4 payload that passes the lane's stateless rules (the devnet bundle's), and its prefix-state twin.
    fn v4_payload() -> PalwFpCommitmentTxPayloadV3 {
        let ids: Vec<u32> = (0..PROMPT).collect();
        let job = PalwFreePromptJobV3 {
            version: PALW_FP_V4_VERSION,
            network_domain: net(),
            class_id: h64(1),
            executor_bond: TransactionOutpoint { transaction_id: TransactionId::from_u64_word(7), index: 0 },
            executor_pubkey: vec![7; 32],
            operator_id: h64(0xE0),
            anchor_block: h64(0xA0),
            anchor_daa: 5_000,
            job_nonce: [0x11; 32],
            tokenizer_id: h64(0x70),
            prompt_token_ids_hash: crate::palw_v2::prompt_token_ids_hash_v2(&ids),
            prompt_tokens: PROMPT,
            decode_token_limit: 6,
            max_context_tokens: 4_096,
            privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
            prompt_mode: crate::palw_freeprompt_v3::PALW_FP_PROMPT_MODE_USER,
            sampling_seed: crate::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
            temperature_q: crate::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
            decode: Some(DecodeConfigV4::NOOP),
            tail: None,
        };
        let decode = 4u32;
        let events: Vec<Hash64> = (0..decode as u64).map(|i| h64(i + 1)).collect();
        let (manifest_root, chunk_count, _) = fp_trace_manifest_v3(h64(0xB1), &events);
        PalwFpCommitmentTxPayloadV3 {
            version: PALW_FP_V3_VERSION,
            commitment: PalwFreePromptCommitmentV3 {
                trace_root: h64(0x7A),
                output_root: h64(0x0B),
                execution_root: h64(0x4E),
                schedule_root: h64(0x5C),
                decode_tokens_executed: decode,
                stop_reason: PalwFpStopReasonV3::EndOfGeneration,
                work_leaves: (PROMPT as u64 + decode as u64) * 64,
                trace_manifest_root: manifest_root,
                trace_chunk_count: chunk_count,
                trace_retention_daa: 505_000,
                job,
            },
            prompt_token_ids: ids,
            signature: vec![0x5A; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
        }
    }

    fn prefix_payload() -> PalwFpCommitmentTxPayloadV3 {
        let mut p = v4_payload();
        p.commitment.job.version = PALW_FP_PREFIX_VERSION;
        p.commitment.job.tail = Some(PalwFpJobTailV1::Prefix(state()));
        p
    }

    fn tx(bytes: Vec<u8>) -> Transaction {
        Transaction::new(TX_VERSION, vec![], vec![], 0, SUBNETWORK_ID_PALW_FP_COMMITMENT, 0, bytes)
    }

    fn walk(p: &PalwFpCommitmentTxPayloadV3, armed: bool, tokenizer: PalwFpTokenizerRuleV1) -> crate::palw_fp_objects_v3::PalwFpExtractionV3 {
        palw_fp_objects_from_accepted_txs_by_class_v1(
            &[tx(borsh::to_vec(p).unwrap())],
            net(),
            &freeprompt(),
            crate::BlockHash::default(),
            false,
            |_| PalwFpClassCapsV1 {
                step_ladder: 1 << 26,
                held: false,
                derived_work: PalwFpDerivedWorkCapV1::Declared,
                logits_q24: true,
                prefix_state_armed: armed,
                prefix_inherit_armed: false, prefix_inherit_class_safe: false,
                constraint_armed: false,
                constraint_v2_armed: false,
                tokenizer,
            },
            false,
            false,
            crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            PalwFpDecodeRulesV1::Active,
            |_, _, _, _| true,
        )
    }

    /// **The wire: a V4 job's bytes with the version word 11 and the prefix state after its decode rules**; the job is
    /// its own id, no V3/V4 rule admits it, and a V4 payload names no state.
    #[test]
    fn a_prefix_state_job_is_a_v4_job_with_the_state_after_its_decode_rules() {
        let (v4, v11) = (v4_payload(), prefix_payload());
        let (b4, b11) = (borsh::to_vec(&v4.commitment.job).unwrap(), borsh::to_vec(&v11.commitment.job).unwrap());
        assert_eq!(&b11[..2], &PALW_FP_PREFIX_VERSION.to_le_bytes());
        assert_eq!(&b11[2..b4.len()], &b4[2..], "every V3 field and the decode rules are the V4 job's, byte for byte");
        assert_eq!(&b11[b4.len()..], &borsh::to_vec(&state()).unwrap()[..], "then the prefix state");
        let back: PalwFreePromptJobV3 = borsh::from_slice(&b11).unwrap();
        assert_eq!(back, v11.commitment.job);
        assert_eq!(palw_fp_prefix_tail_v1(&back), Some(&state()));
        assert_ne!(fp_job_id_v3(&v11.commitment.job), fp_job_id_v3(&v4.commitment.job), "its id is under its own key");
        assert_eq!(fp_job_id_v3(&v11.commitment.job), fp_job_id_prefix_v1(&v11.commitment.job));
        assert_ne!(fp_claim_id_v3(&v11.commitment), fp_claim_id_v3(&v4.commitment), "the state is inside the claim id");
        let mut moved = v11.clone();
        moved.commitment.job.tail = Some(PalwFpJobTailV1::Prefix(PalwFpPrefixStateV1 { state_root: h64(0x52), ..state() }));
        assert_ne!(fp_claim_id_v3(&moved.commitment), fp_claim_id_v3(&v11.commitment), "a relayer cannot move the state");
        // The state the payload names: the tail's for version 11, genesis for every other.
        assert_eq!(v11.consumed_prefix_state_v1(), state());
        assert!(v4.consumed_prefix_state_v1().is_genesis());
        // No V3/V4 rule admits version 11; V4 bytes do not decode as a prefix-state job.
        for rules in [PalwFpDecodeRulesV1::Dormant, PalwFpDecodeRulesV1::Scheduled, PalwFpDecodeRulesV1::Active] {
            assert!(matches!(
                rules.check_job(&v11.commitment.job),
                Err(PalwFpV3Error::UnsupportedVersion { got: PALW_FP_PREFIX_VERSION, .. })
            ));
        }
        assert!(palw_fp_prefix_tail_v1(&v4.commitment.job).is_none());
        assert!(palw_fp_payload_is_prefix_v1(&borsh::to_vec(&v11).unwrap()) && !palw_fp_payload_is_prefix_v1(&borsh::to_vec(&v4).unwrap()));
    }

    #[test]
    fn a_malformed_prefix_state_job_is_refused_by_name() {
        let ok = prefix_payload().commitment.job;
        assert_eq!(palw_fp_prefix_shape_v1(&ok), Ok(state()));
        let mut job = ok.clone();
        job.version = PALW_FP_V4_VERSION;
        assert_eq!(palw_fp_prefix_shape_v1(&job), Err(PalwFpPrefixErrorV1::NotAPrefixJob { version: PALW_FP_V4_VERSION }));
        let mut job = ok.clone();
        job.decode = None;
        assert_eq!(palw_fp_prefix_shape_v1(&job), Err(PalwFpPrefixErrorV1::NoDecode));
        let mut job = ok.clone();
        job.tail = None;
        assert_eq!(palw_fp_prefix_shape_v1(&job), Err(PalwFpPrefixErrorV1::NoTail));
        for genesis in [
            PalwFpPrefixStateV1 { state_root: Hash64::default(), ..state() },
            PalwFpPrefixStateV1 { prefix_tokens: 0, ..state() },
        ] {
            let mut job = ok.clone();
            job.tail = Some(PalwFpJobTailV1::Prefix(genesis));
            assert_eq!(palw_fp_prefix_shape_v1(&job), Err(PalwFpPrefixErrorV1::StateIsGenesis));
        }
        let mut job = ok.clone();
        job.tail = Some(PalwFpJobTailV1::Prefix(PalwFpPrefixStateV1 { class_id: h64(9), ..state() }));
        assert_eq!(palw_fp_prefix_shape_v1(&job), Err(PalwFpPrefixErrorV1::StateClassMismatch));
        for prefix in [PROMPT, PROMPT + 5] {
            let mut job = ok.clone();
            job.tail = Some(PalwFpJobTailV1::Prefix(PalwFpPrefixStateV1 { prefix_tokens: prefix, ..state() }));
            assert_eq!(
                palw_fp_prefix_shape_v1(&job),
                Err(PalwFpPrefixErrorV1::PrefixNotBelowPrompt { prefix, prompt: PROMPT }),
                "the prefix must leave a new position"
            );
        }
    }

    /// **The walk: skipped below the fence, carried with its state from it**, judged on its V4 stand-in; a V4 claim is
    /// unaffected either side.
    #[test]
    fn the_walk_carries_the_state_past_the_fence_and_skips_the_claim_below_it() {
        let dormant = PalwFpTokenizerRuleV1::Dormant;
        let below = walk(&prefix_payload(), false, dormant);
        assert!(below.objects.is_empty());
        assert_eq!(below.skipped[0].1, "a prefix-state claim (FP job version 11) below palw_fp_prefix_state");
        let armed = walk(&prefix_payload(), true, dormant);
        assert!(armed.skipped.is_empty(), "{:?}", armed.skipped);
        let [carrier] = armed.objects.as_slice() else { panic!("one object") };
        let PalwConsensusObjectV2::FreePromptCommitted { consumed_prefix_state, claim, prompt_tokens, .. } = &carrier.object else {
            panic!("a free-prompt claim")
        };
        assert_eq!(*consumed_prefix_state, state(), "the fold prices what the signer named");
        assert_eq!(*claim, fp_claim_id_v3(&prefix_payload().commitment));
        assert_eq!(*prompt_tokens, PROMPT);
        // A malformed state is skipped, not carried.
        let mut genesis = prefix_payload();
        genesis.commitment.job.tail = Some(PalwFpJobTailV1::Prefix(PalwFpPrefixStateV1 { prefix_tokens: 0, ..state() }));
        let skipped = walk(&genesis, true, dormant);
        assert!(skipped.objects.is_empty());
        assert_eq!(skipped.skipped[0].1, "a prefix-state claim whose job is not well formed");
        // A V4 claim names no state, armed or not.
        for flag in [false, true] {
            let v4 = walk(&v4_payload(), flag, dormant);
            let [carrier] = v4.objects.as_slice() else { panic!("one object") };
            let PalwConsensusObjectV2::FreePromptCommitted { consumed_prefix_state, .. } = &carrier.object else { panic!() };
            assert!(consumed_prefix_state.is_genesis());
        }
        // The stand-in runs every V4 rule: a prefix claim whose ids do not hash to the job is skipped like a V4 one.
        let mut wrong_ids = prefix_payload();
        wrong_ids.prompt_token_ids[0] ^= 1;
        assert!(walk(&wrong_ids, true, dormant).objects.is_empty());
    }

    /// RFC-0001 §2.9 at the walk, for any job version: past the fence a class that lists a tokenizer admits only it.
    #[test]
    fn the_tokenizer_rule_skips_a_job_that_is_not_its_classs_listed_one() {
        let listed_other = PalwFpTokenizerRuleV1::of(true, Some(h64(0x99)));
        let skipped = walk(&v4_payload(), false, listed_other);
        assert!(skipped.objects.is_empty());
        assert!(skipped.skipped[0].1.contains("palw_fp_tokenizer_match"), "{:?}", skipped.skipped);
        assert_eq!(walk(&v4_payload(), false, PalwFpTokenizerRuleV1::of(true, Some(h64(0x70)))).objects.len(), 1, "its own passes");
        assert_eq!(walk(&v4_payload(), false, PalwFpTokenizerRuleV1::of(true, None)).objects.len(), 1, "a class that lists none is not asked");
        assert_eq!(walk(&v4_payload(), false, PalwFpTokenizerRuleV1::Dormant).objects.len(), 1, "dormant: not asked");
    }

    /// **The doors**: isolation by the ruleset's schedule, the header context by the height.
    #[test]
    fn the_doors_open_for_a_prefix_state_claim_only_where_the_ruleset_and_the_height_say() {
        let bytes = borsh::to_vec(&prefix_payload()).unwrap();
        let door = |prefix_door: bool| {
            validate_palw_fp_commitment_tx_under_v8(
                &bytes,
                false,
                PalwPromptIdsFormV1::Flat,
                1 << 32,
                PalwFpDecodeRulesV1::Scheduled,
                false,
                false,
                prefix_door,
                true,
            )
        };
        assert!(matches!(door(false), Err(PalwFpV3Error::UnsupportedVersion { got: 11, .. })), "shut: refused like a build without it");
        assert_eq!(door(true), Ok(()));
        // A malformed one is refused by name at the open door.
        let mut genesis = prefix_payload();
        genesis.commitment.job.tail = Some(PalwFpJobTailV1::Prefix(PalwFpPrefixStateV1 { prefix_tokens: 0, ..state() }));
        let refused = validate_palw_fp_commitment_tx_under_v8(
            &borsh::to_vec(&genesis).unwrap(),
            false,
            PalwPromptIdsFormV1::Flat,
            1 << 32,
            PalwFpDecodeRulesV1::Scheduled,
            false,
            false,
            true,
            true,
        );
        assert!(matches!(refused, Err(PalwFpV3Error::PrefixStateClaim(ref why)) if why.contains("genesis")), "{refused:?}");
        // Every other payload goes to the door under it unchanged.
        let v4 = borsh::to_vec(&v4_payload()).unwrap();
        for prefix_door in [false, true] {
            assert_eq!(
                validate_palw_fp_commitment_tx_under_v8(
                    &v4,
                    false,
                    PalwPromptIdsFormV1::Flat,
                    1 << 32,
                    PalwFpDecodeRulesV1::Scheduled,
                    false,
                    false,
                    prefix_door,
                    true
                ),
                Ok(())
            );
        }
        // The header-context half.
        assert_eq!(palw_fp_prefix_refusal_at_v1(&bytes, false, true, true), Some(PalwFpPrefixHeightRefusalV1::BelowPrefixState));
        assert_eq!(palw_fp_prefix_refusal_at_v1(&bytes, true, false, true), Some(PalwFpPrefixHeightRefusalV1::BelowDecodeRules));
        assert_eq!(palw_fp_prefix_refusal_at_v1(&bytes, true, true, true), None);
        assert_eq!(palw_fp_prefix_refusal_at_v1(&v4, false, false, false), None, "a V4 payload is not this door's");
    }

    #[test]
    fn a_state_root_binds_the_class_the_length_and_the_state() {
        let (class, kv) = (h64(1), h64(2));
        let base = palw_fp_prefix_state_root_v1(&class, 8, &kv);
        assert_ne!(base, palw_fp_prefix_state_root_v1(&h64(9), 8, &kv));
        assert_ne!(base, palw_fp_prefix_state_root_v1(&class, 9, &kv));
        assert_ne!(base, palw_fp_prefix_state_root_v1(&class, 8, &h64(3)));
        assert_eq!(base, palw_fp_prefix_state_root_v1(&class, 8, &kv));
        assert_ne!(base, Hash64::default());
    }
}
