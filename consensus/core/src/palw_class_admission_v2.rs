//! **The gate a class must pass to join a chain that is already running** (ADR-0039's
//! "weightless until the catalog closes", used in the direction it was written for).
//!
//! # Why this exists
//!
//! `verify_palw_genesis_v2` checks the class set a network is born with, against a catalog whose
//! root is inside `PalwConsensusParamsV2` and therefore inside `palw_ruleset_id_v2`. That is the
//! right shape for genesis and the wrong shape for later: a class that does not exist yet has no
//! `artifact_root`, so it cannot be in the genesis catalog, so under the genesis gate alone **a
//! second class is a flag day** — a new ruleset id, a coordinated upgrade, a different network.
//!
//! The plan the RC is built on is the opposite: ship the BASE-0 liveness floor now, add a larger
//! class once its weights and its PTQ pipeline exist. This module is the missing half of that —
//! the checks the genesis loader runs, restated so they can run against ONE registration and the
//! shape profile it carries, with no pre-committed catalog to read from.
//!
//! # Derive, never declare
//!
//! The genesis catalog can afford to state `reachable_kernels`, `canonical_step_leaf_count` and
//! `max_step_leaf_count`, because its root is hashed into the ruleset and an operator who lies
//! contradicts a commitment the chain already made. A post-genesis registration has no such
//! anchor, so **nothing here is read from the registration that can be computed from the graph**:
//! the reachable set comes from the profile's own nodes, both leaf counts come from
//! `palw_step`, and the entry this returns is a derivation rather than a copy. The only fields a
//! registrant supplies are the ones no function can invent — `artifact_root` (the weights) and the
//! economic terms — and `pwu_per_inference`, which is checked against the count rather than
//! trusted.
//!
//! # What this does NOT do
//!
//! It does not admit anything. There is no carrier for `ClassRegistered` outside the genesis
//! object list in this tree, so this module is consensus-inert: nothing calls it, exactly like
//! every other V2 brick before its wiring lands. When the carriage does carry a registration, this
//! is the function it must call before the transition runs — and the transition is deliberately
//! left alone, because `apply_palw_transition_v2` is a pure state machine and adjudicability is an
//! arithmetic fact about a graph, not a fact about state.
//!
//! Nor does it decide share. `granted_share_table_v2` owns that, and it refuses a zero grant by
//! construction (`min_grantable_share_permille` is at least 1): a class with no share has a zero
//! epoch budget, which is a class that can never mine. So "register it weightless and activate it
//! later" is not available, and the honest version of the plan is **register it at the minimum
//! grantable share** — one permille, donated from the incumbents, which is the smallest weight the
//! ruleset admits rather than none.

use std::collections::BTreeSet;

use kaspa_hashes::Hash64;

use crate::palw_catalog_coverage::{PalwReachableKernelSetV1, verify_catalog_coverage_v1};
use crate::palw_mode_v2::{PalwClassCatalogEntryV2, PalwConsensusParamsV2};
use crate::palw_state_v2::{PalwConsensusObjectV2, PalwPwuRuleV2};
use crate::palw_step::{PalwShapeProfileV3, step_leaf_count, worst_case_step_leaf_count_v1};
use crate::palw_step_refute::catalogued_kernel_ids_v1;
use crate::palw_v2::PalwJobContextV2;

/// **The `max_step_leaf_count` a network must freeze at genesis to keep a second class possible.**
///
/// `PalwCourtParamsV2::max_step_leaf_count` is a `PalwConsensusParamsV2` field, and the bundle is
/// what `palw_ruleset_id_v2` hashes. A class whose worst case is deeper than the ladder therefore
/// cannot join a running chain at all — it needs a new ruleset, which is a flag day. Unlike every
/// other obstacle to adding a class later, **this one cannot be repaired later**: by the time the
/// second class exists, the number is already inside the network's identity.
///
/// Provisioning it at the step space's own cap costs almost nothing, because the ladder is
/// `ceil(log2(leaves)) + terminal` ROUNDS. Measured on this tree
/// (`misaka-palw-base0/src/bin/base0-class-sizing.rs`), and pinned by
/// `provisioning_the_whole_step_space_costs_four_rounds`:
///
/// | provisioned for | leaves | bisection rounds |
/// |---|---|---|
/// | the RC floor alone | 184,456 | 18 |
/// | the whole step space | 4,194,304 | 22 |
///
/// The floor's figure is its WHOLE CONTEXT as prefill (`worst_case_step_leaf_count_v1`), not the
/// 47,020 of its declared 64/64 job — the ladder must reach the longest job a class admits, or it
/// admits a class an attacker picks the job length for. An earlier draft of this table used the
/// declared job and put the price at six rounds; the test below is what corrected it.
///
/// Four extra rounds of worst-case prosecution — paid only when a court actually runs to its worst
/// case — buys every class that could ever be adjudicable, because nothing deeper than
/// `PALW_STEP_MAX_LEAVES` is admissible in the first place (`worst_case_step_leaf_count_v1`
/// refuses it).
pub const PALW_RC_COURT_MAX_STEP_LEAF_COUNT: u64 = crate::palw_step::PALW_STEP_MAX_LEAVES;

/// **The three cost ceilings an RC identity must freeze, and why they are constants rather than
/// an operator's choice** (ADR-0049 Decision C; the second of the road map's two decisions that
/// expire).
///
/// `PALW_RC_COURT_MAX_STEP_LEAF_COUNT` above bounds how many rounds a dispute takes. These bound
/// what a round COSTS, and they sit in the same place for the same reason: they are
/// `PalwConsensusParamsV2` fields, so they are inside `palw_ruleset_id_v2`, so a class that exceeds
/// them cannot join a running chain — it needs a new ruleset, which is a flag day. The ladder gate
/// was already a refusal in `assemble_palw_rc_identity_v2`; these were not, and an RC genesis could
/// be minted with any ceiling at all, including the generous default nobody had checked against a
/// transaction.
///
/// They are the shipped defaults, restated here as an RC commitment. Keeping two names is
/// deliberate: [`crate::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES`] is what a bundle gets when a caller
/// does not say, and this is what the RC network's identity IS. A future ruleset may move the
/// default; moving what testnet-11's genesis froze would be a different network.
pub const PALW_RC_COURT_MAX_CLOSE_BYTES: u64 = crate::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES;
/// See [`PALW_RC_COURT_MAX_CLOSE_BYTES`]. The floor's widest step recomputes 32,768
/// multiply-accumulates; this is 512 times that.
pub const PALW_RC_COURT_MAX_TERMINAL_MACS: u64 = crate::palw_mode_v2::DEFAULT_MAX_TERMINAL_MACS;
/// See [`PALW_RC_COURT_MAX_CLOSE_BYTES`]. The floor's widest step reads two operands; a
/// gated-delta-net recurrence reads five.
pub const PALW_RC_COURT_MAX_OPERAND_COUNT: u32 = crate::palw_mode_v2::DEFAULT_MAX_OPERAND_COUNT;

/// **What one terminal adjudication of this class costs, derived from its graph** (ADR-0049
/// Decision C).
///
/// The ladder bounds how many rounds a dispute takes. Nothing bounded what a round COSTS, and the
/// answer used to be the model's size: the matmul arm opened `out_dim x in_len` — the whole matrix —
/// which is ~223 MiB for Qwen2.5-1.5B's unembed against a court-close budget of 152 KB. Decision B
/// made the opening tile-local; this is what turns "small" into "bounded", so a class cannot be
/// admitted whose disputes are unprosecutable in a transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwCourtCostV1 {
    /// Bytes the most expensive single close costs to carry: the artifact opening, the disputed
    /// step's own leaves and input leaves, the generated-token pin where a gather needs one, and
    /// every Merkle path element proving any of them. The units
    /// `palw_court_v2::arithmetic_close_bytes_v2` counts on a real object.
    pub max_close_bytes: u64,
    /// Multiply-accumulates a full node performs to recompute that step — its own CPU, on
    /// peer-supplied input.
    pub max_terminal_macs: u64,
    /// Rows a single step reads: its `input_refs` plus at most one weight operand. Bounds
    /// deserialization work before any arithmetic runs.
    pub max_operand_count: u32,
}

/// Widths a node can produce, worst case, from the profile's own geometry.
///
/// `KvScaled` is resolved at the CONTEXT maximum rather than at some typical position, because a
/// ceiling that held for a typical job and not for the longest one would admit a class an attacker
/// picks the job length for — the same rule `worst_case_step_leaf_count_v1` states.
fn node_out_width_v1(node: &crate::palw_step::PalwStepNodeV1, profile: &PalwShapeProfileV3) -> Option<u64> {
    match node.out_len {
        crate::palw_step::PalwStepOutLenV1::Fixed { elements } => Some(elements as u64),
        crate::palw_step::PalwStepOutLenV1::KvScaled { multiplier } => (multiplier as u64).checked_mul(profile.n_ctx as u64),
    }
}

/// The width of one input reference: a sentinel's own width, or an earlier node's output.
fn input_width_v1(r: u16, table: &[crate::palw_step::PalwStepNodeV1], profile: &PalwShapeProfileV3) -> Option<u64> {
    use crate::palw_step::{PALW_STEP_INPUT_CHECKPOINT_STATE, PALW_STEP_INPUT_KV_K, PALW_STEP_INPUT_KV_V, PALW_STEP_INPUT_LAYER_IN};
    let kv_dim = (profile.attn_kv_heads as u64).checked_mul(profile.attn_head_dim as u64)?;
    match r {
        PALW_STEP_INPUT_LAYER_IN => Some(profile.hidden_dim as u64),
        PALW_STEP_INPUT_KV_K | PALW_STEP_INPUT_KV_V => kv_dim.checked_mul(profile.n_ctx as u64),
        // The recurrent state chunk. Bounded by the widest state a GDN head can carry; a profile
        // with no GDN reaches this only if it names the sentinel, and then zero is the honest width.
        PALW_STEP_INPUT_CHECKPOINT_STATE => {
            (profile.gdn_heads as u64).checked_mul(profile.gdn_head_k_dim as u64)?.checked_mul(profile.gdn_head_v_dim as u64)
        }
        // An intra-table index: references are backwards, so the node exists by validation.
        i => table.get(i as usize).and_then(|n| node_out_width_v1(n, profile)),
    }
}

/// **How many Merkle path elements an ARTIFACT opening can carry.**
///
/// `open_artifact_leaf_v1` addresses a leaf by `u32`, so an inventory has at most `2^32` leaves
/// however finely a registrant tiles its weights, and a path over them is at most 32 elements.
/// Derived from the format rather than from the inventory, because the inventory is the
/// registrant's and this is a bound on what the registrant may do.
pub const PALW_ARTIFACT_MAX_PATH_ELEMENTS: u64 = 32;

/// A Merkle path element is one `Hash64`.
const PATH_ELEMENT_BYTES: u64 = 64;

/// The tile length of whichever node a step's input reference resolves to — that is the tiling the
/// input openings are cut on, and it is not this node's own.
fn source_tile_len_v1(table: &[crate::palw_step::PalwStepNodeV1], node: &crate::palw_step::PalwStepNodeV1, r: u16) -> u64 {
    use crate::palw_step::{PALW_STEP_INPUT_KV_K, PALW_STEP_INPUT_KV_V, PalwStepNodeRoleV1};
    let by_role = |want: PalwStepNodeRoleV1| table.iter().find(|n| n.role == want).map(|n| n.tile_len as u64);
    match r {
        PALW_STEP_INPUT_KV_K => by_role(PalwStepNodeRoleV1::KCacheWrite),
        PALW_STEP_INPUT_KV_V => by_role(PalwStepNodeRoleV1::VCacheWrite),
        i if (i as usize) < table.len() => Some(table[i as usize].tile_len as u64),
        _ => None,
    }
    .unwrap_or(node.tile_len as u64)
    .max(1)
}

/// **What the generated-token pin costs, by the scheme the CLASS committed to** (ADR-0049
/// Decision E, and the tiled scheme that followed it).
///
/// `logits_scheme_id` is a field of `PalwShapeProfileV3`, so it is inside `shape_profile_id` and
/// therefore inside the class id: a class cannot change how its decode tokens are pinned without
/// becoming a different class. That is what makes this dispatchable at all. The earlier shape —
/// a `trace_scheme_id` on the job context — would have let a class register under the cheap scheme
/// and produce under the expensive one, which is "an attacker picks the job length" with the
/// scheme substituted for the length.
///
/// The two prices are the two the court's own cost gate counts on a real object
/// (`check_close_cost_v2`), restated from the geometry:
///
/// * **flat** — `base0_logits_trace_root_v1` is a hash over every row, so recomputing it needs all
///   of them: `decode x vocabulary x 4`. At Qwen3.6's 248,320 lanes ONE row is 993 KB.
/// * **tiled** — two tile openings, three Merkle paths and the ids. `PALW_LOGITS_TILE_LANES` is
///   4,096, so a tile is 16 KiB and the paths are `ceil(log2(vocab / lanes))` deep within a row
///   plus `ceil(log2(decode))` over the rows tree.
///
/// An unrecognised scheme is priced at the most expensive one this function knows. Admission
/// refuses such a class outright, so this is belt and braces — but the belt has to fail closed,
/// because a scheme nobody can price is not a scheme nobody can register.
fn decode_pin_price_v1(profile: &PalwShapeProfileV3, decode: u64) -> Option<u64> {
    // The float lane's pin is a trace SUMMARY and the ids; the rows stay in the event tree the
    // summary roots, so there is nothing here for it to carry.
    if profile.lane == crate::palw_step::PalwStepLaneV1::Float32 {
        return Some(0);
    }
    let vocab = profile.vocab_size as u64;
    let flat = decode.checked_mul(vocab)?.checked_mul(4)?;
    if profile.logits_scheme_id != crate::palw_step_refute::tiled_logits_scheme_id_v1() {
        return Some(flat);
    }
    let lanes = crate::palw_step_refute::PALW_LOGITS_TILE_LANES as u64;
    let tile_bytes = lanes.min(vocab.max(1)).checked_mul(4)?;
    let ceil_log2 = |n: u64| -> u64 { if n <= 1 { 0 } else { u64::from((n - 1).next_power_of_two().trailing_zeros()) } };
    let within_row = ceil_log2(vocab.div_ceil(lanes));
    let across_rows = ceil_log2(decode);
    let paths = within_row.checked_mul(2)?.checked_add(across_rows)?.checked_mul(64)?;
    // Two tiles (the committed lane's and the beating lane's), their paths, the row opening's
    // path, and every generated id.
    tile_bytes.checked_mul(2)?.checked_add(paths)?.checked_add(decode.checked_mul(4)?)
}

/// The cost of the most expensive step this profile can be challenged at.
///
/// Every quantity is read off the graph. Nothing is declared, so a registration cannot understate
/// what prosecuting it will cost — which is the same reason `pwu_per_inference` is checked against
/// a count rather than trusted.
///
/// # It measures the CLOSE, not the weight bytes
///
/// The bound is `max_close_bytes` and the units are the ones
/// `palw_court_v2::arithmetic_close_bytes_v2` counts on a real object: opened payload plus every
/// Merkle path element proving it, artifact side and step side alike. That matters because the
/// step side is the larger one. A `MatMulQuant` at an attention site opens no weight at all and
/// reads the whole KV HISTORY — `n_ctx` positions, each cut into tiles, each tile carrying its own
/// path — so the shipped floor's most expensive close at a 64/64 job is 750,716 bytes beside a
/// derived 32,768. Deriving the weight bytes alone said "32 KiB" about a close no block could
/// carry.
///
/// # Over the class's LONGEST job
///
/// Path length comes from `worst_case_step_leaf_count_v1` and the KV history from `n_ctx`, for the
/// reason `node_out_width_v1` already gives: a ceiling that held for a typical job and not for the
/// longest one would admit a class an attacker picks the job length for. Nothing charges an
/// attempt more for a longer job — `pwu_per_inference` is per inference — so the longest job the
/// class admits is a job the class must be prosecutable at.
pub fn derive_court_cost_v1(profile: &PalwShapeProfileV3) -> Result<PalwCourtCostV1, PalwClassAdmissionError> {
    derive_court_cost_shaped_v1(profile, PalwCourtCostShapeV1::genesis_anchored_v1(profile))
}

/// **Which court a cost is being derived FOR** (ADR-0077 Decision 11).
///
/// Every field here is a property of the RULESET, not of the class: how deep the ladder the
/// network froze is, how far back a refutation has to replay before it may substitute a verified
/// checkpoint, and what that checkpoint's opening costs. The class's own geometry stays where it
/// was — read off the graph, never declared — so this parameterises the question and not the
/// answer.
///
/// [`Self::genesis_anchored_v1`] is what testnet-11 runs today and what
/// [`derive_court_cost_v1`] passes, so the shipped derivation is byte-identical to the one this
/// split replaced. [`Self::checkpoint_anchored_v1`] is Decision 11's form, reachable only behind
/// `Params::palw_context_ladder`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwCourtCostShapeV1 {
    /// How many POSITIONS a history-reading reference opens. The whole context for the
    /// genesis-anchored form (a dispute at a prefill position has no anchor to stand on); at most
    /// the checkpoint interval for the anchored one.
    pub history_positions: u64,
    /// The ladder top the step path is measured against — `PalwCourtParamsV2::max_step_leaf_count`
    /// in the bundle this class would join.
    pub ladder: u64,
    /// What ONE checkpoint-chunk opening of the KV CACHE costs, added once per history-reading
    /// reference of an attention node. Zero for the genesis-anchored form, which opens no
    /// checkpoint because there is none to open.
    ///
    /// Separate from [`Self::gdn_checkpoint_bytes`] because the two anchors are different KINDS of
    /// object and pricing them as one would hide the finding: a recurrence has a state whose size
    /// does not depend on the context, and a cache is the history itself. See
    /// `crate::palw_context_ladder::palw_kv_checkpoint_opening_bytes_v1`.
    pub kv_checkpoint_bytes: u64,
    /// What ONE checkpoint-chunk opening of the RECURRENCE state costs. Constant in `n_ctx` — the
    /// half of ADR-0077 Decision 11 that actually buys a wider row.
    pub gdn_checkpoint_bytes: u64,
    /// **Whose depth a step leaf's Merkle path is measured at**: the CLASS's own worst case
    /// (`false`, what testnet-11 does) or the RULESET's ladder (`true`, ADR-0077 Decision 12).
    ///
    /// The shipped form asks how deep this class's own tree can get, which is exact and which is
    /// also `⌈log₂ n_ctx⌉`-shaped: doubling the context adds one `Hash64` to every opened run's
    /// path, and with a few hundred runs on a close that is kilobytes. It is the residue that made
    /// a first reading of W1 fail — the history term was flat and the PATHS were not.
    ///
    /// Past the fence the path is budgeted at the ladder the ruleset froze — Decision 12's own
    /// sentence, "a Merkle path grows to 32 elements — 2 KiB per opened leaf — inside the close
    /// budget". That is a bound rather than a measurement, it can only over-charge a narrow class,
    /// and it is what makes a mapped class's price genuinely independent of its context: the one
    /// property Decision 11 exists to buy.
    pub path_from_ladder: bool,
    /// Whether the prompt-id and generated-token-pin terms are counted.
    ///
    /// Always `true` for any cost a gate reads: those bytes ride real closes. The `false` reading
    /// exists so a test can ask the question Decision 11 actually answers — *is the HISTORY term
    /// flat in `n_ctx`* — without the id term, which Decision 11 does not anchor, standing in
    /// front of the answer (ADR-0077 §4 budgets those separately: "PublicDa carries `n_ctx × 4`
    /// bytes of ids").
    pub count_ids: bool,
    /// **Which form the job's `prompt_token_ids_hash` takes** (ADR-0081 Decision 3), and therefore
    /// what the prompt-id term below costs: the whole list, or one opening of it.
    ///
    /// A property of the RULESET like every other field here — `Params::palw_prompt_ids_form_at`,
    /// which is `Flat` on every shipped preset. Both constructors below say `Flat` explicitly, so
    /// this split changes no shipped price; `the_prompt_id_term_is_the_openings_size_past_the_fence`
    /// is what says the other reading is a reading and not a rewrite.
    pub prompt_ids_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    /// **Whether a fused attention leaf is tried by DISSECTION, and at what arity** (ADR-0082
    /// Decisions 2 and 3).
    ///
    /// `None` — every court built before ADR-0082 — prices a fused site by the WHOLE-ROW route:
    /// the query row plus the K and V series over [`Self::history_positions`], which is the same
    /// run the four separate nodes are charged today and is design A's width. `Some(k)` prices it
    /// by Decision 2's bottom opening plus the widest disclosure one `k`-ary move carries, which is
    /// flat in `n_ctx`.
    ///
    /// A property of the RULESET like every field beside it: `k` is
    /// `PalwCourtParamsV2::dissection_arity`, inside `palw_ruleset_id_v2`, and whether the arm is
    /// admissible at all is `Params::palw_kary_court`. The fence is never read from inside the
    /// walk — a cost derivation that consulted a DAA score would price one class two ways
    /// depending on when it was asked. The caller reads the fence and says.
    pub dissection: Option<u8>,
}

/// **What a `palw_kary_court`-armed ruleset has turned on** (ADR-0082 Decisions 3 and 5).
///
/// Both fields are read off the ruleset by the CALLER — `dissection_arity` from
/// `PalwCourtParamsV2`, the id form from `Params::palw_prompt_ids_form_at` — and neither is
/// guessed here. They travel together because Decision 5 arms the Merkle prompt ids "in the same
/// ruleset move as the rows", and because the two move the price in OPPOSITE directions: the
/// dissection makes the attention term flat and the Merkle form makes the id term logarithmic, so
/// a derivation that assumed one from the other would either over-charge (safe) or under-charge (
/// the direction that admits a class whose disputes nobody can carry). Stating both makes which
/// one a caller armed a fact rather than an inference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwKaryCourtV1 {
    /// `PalwCourtParamsV2::dissection_arity` — a power of two in `2..=64`.
    pub dissection_arity: u8,
    /// `Params::palw_prompt_ids_form_at` at the block the registration is judged in.
    pub prompt_ids_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    /// `window_court` from the lattice this ruleset runs — what the whole dispute must fit inside
    /// (ADR-0082 Decision 3, Z4/Z11). In DAA.
    pub window_court_daa: u64,
}

impl PalwCourtCostShapeV1 {
    /// The shipped court: the whole context as history, the shipped ladder, no anchor.
    pub fn genesis_anchored_v1(profile: &PalwShapeProfileV3) -> Self {
        Self {
            history_positions: profile.n_ctx as u64,
            ladder: crate::palw_step::PALW_STEP_MAX_LEAVES,
            kv_checkpoint_bytes: 0,
            gdn_checkpoint_bytes: 0,
            path_from_ladder: false,
            count_ids: true,
            prompt_ids_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            dissection: None,
        }
    }

    /// Decision 11's court: `min(n_ctx, interval)` positions of history, one checkpoint-chunk
    /// opening per history-reading reference, against the caller's ladder. The two opening prices
    /// start at zero and the caller fills in whichever apply — `palw_context_ladder` derives both
    /// from the profile's own map.
    pub fn checkpoint_anchored_v1(profile: &PalwShapeProfileV3, interval: u32, ladder: u64, checkpoint_bytes: u64) -> Self {
        Self {
            history_positions: (profile.n_ctx as u64).min(u64::from(interval).max(1)),
            ladder,
            kv_checkpoint_bytes: checkpoint_bytes,
            gdn_checkpoint_bytes: checkpoint_bytes,
            path_from_ladder: true,
            count_ids: true,
            prompt_ids_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            dissection: None,
        }
    }

    /// The same court, reading the prompt ids under ADR-0081 Decision 3's form. Separate from the
    /// two constructors above rather than a parameter of them, because the form is the one thing
    /// here a network can arm on its own fence (`Params::palw_prompt_ids_merkle`) while every other
    /// field stays exactly what the ruleset already froze.
    pub fn with_prompt_ids_form_v1(mut self, form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1) -> Self {
        self.prompt_ids_form = form;
        self
    }

    /// **The dissection court** (ADR-0082 Decisions 2 and 3): the same anchored shape, with a
    /// fused attention leaf priced by its bottom opening and one move rather than by the history
    /// it never carries. Separate from the constructors above for the reason
    /// [`Self::with_prompt_ids_form_v1`] is separate: the arity is the one field here a running
    /// network arms on its own fence (`Params::palw_kary_court`) while every other stays what the
    /// ruleset already froze.
    pub fn with_dissection_v1(mut self, arity: u8) -> Self {
        self.dissection = Some(arity);
        self
    }
}

/// [`derive_court_cost_v1`] against a stated court (ADR-0077 Decision 11).
///
/// One enumeration answers every form of the question, which is the point: a second walk that
/// merely happened to agree with this one is how a class gets admitted at one price and prosecuted
/// at another.
pub fn derive_court_cost_shaped_v1(
    profile: &PalwShapeProfileV3,
    shape: PalwCourtCostShapeV1,
) -> Result<PalwCourtCostV1, PalwClassAdmissionError> {
    derive_court_cost_walk_v1(profile, shape, &mut None)
}

/// **One node's row of the derivation** — which node bound the class, and at what.
///
/// A `max` tells you the number and never which term produced it, so every reading of "the 512 row
/// costs N" had to be re-derived by hand against the source to find out WHICH node was over. This
/// is that walk's own answer, from the same walk: [`derive_court_cost_shaped_v1`] and
/// [`derive_court_cost_rows_v1`] are one function with the sink absent or present, so a breakdown
/// can never disagree with the total it explains.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwCourtCostRowV1 {
    /// `pre` / `gdn` / `attn` / `post`.
    pub table: &'static str,
    /// The node's index within that table.
    pub index: usize,
    pub op_kind: crate::palw_step::PalwStepOpKindV1,
    pub weight_name: String,
    /// Bytes: the artifact opening this node pays.
    pub opening_bytes: u64,
    /// Bytes: its own tile, every opened input run, the id terms and any checkpoint charge.
    pub evidence_bytes: u64,
    /// `opening_bytes + evidence_bytes` — what this node contributes to `max_close_bytes`.
    pub close_bytes: u64,
    pub terminal_macs: u64,
}

/// [`derive_court_cost_shaped_v1`]'s walk, reported node by node, largest close first.
pub fn derive_court_cost_rows_v1(
    profile: &PalwShapeProfileV3,
    shape: PalwCourtCostShapeV1,
) -> Result<Vec<PalwCourtCostRowV1>, PalwClassAdmissionError> {
    let mut sink = Some(Vec::new());
    derive_court_cost_walk_v1(profile, shape, &mut sink)?;
    let mut rows = sink.expect("the sink was supplied");
    rows.sort_by(|a, b| b.close_bytes.cmp(&a.close_bytes));
    Ok(rows)
}

fn derive_court_cost_walk_v1(
    profile: &PalwShapeProfileV3,
    shape: PalwCourtCostShapeV1,
    sink: &mut Option<Vec<PalwCourtCostRowV1>>,
) -> Result<PalwCourtCostV1, PalwClassAdmissionError> {
    use crate::palw_step::{PALW_STEP_INPUT_KV_K, PALW_STEP_INPUT_KV_V, PalwStepOpKindV1 as Op};
    let over = || PalwClassAdmissionError::Profile("the class's court cost overflows a u64".to_string());
    let mut cost = PalwCourtCostV1 { max_close_bytes: 0, max_terminal_macs: 0, max_operand_count: 0 };

    // The deepest step tree this class can be disputed in, and therefore the longest path any one
    // step leaf can carry.
    let worst_leaves = crate::palw_step::worst_case_step_leaf_count_capped_v1(profile, shape.ladder)
        .map_err(|e| PalwClassAdmissionError::Profile(format!("{e:?}")))?;
    // The class's own depth, or the ruleset's — see `PalwCourtCostShapeV1::path_from_ladder`.
    let path_depth_of = if shape.path_from_ladder { shape.ladder } else { worst_leaves };
    let step_path_bytes = u64::from(path_depth_of.max(2).next_power_of_two().trailing_zeros()) * PATH_ELEMENT_BYTES;
    let kv_dim = (profile.attn_kv_heads as u64).checked_mul(profile.attn_head_dim as u64).ok_or_else(over)?;
    let n_ctx = profile.n_ctx as u64;
    // What a history-reading reference opens, and what it pays once for the right to stop there.
    let history = shape.history_positions;

    for (table_name, table) in
        [("pre", &profile.pre_nodes), ("gdn", &profile.gdn_nodes), ("attn", &profile.attn_nodes), ("post", &profile.post_nodes)]
    {
        for (node_index, node) in table.iter().enumerate() {
            let out_w = node_out_width_v1(node, profile).ok_or_else(over)?;
            let tile = (node.tile_len as u64).min(out_w.max(1));
            let in_w = match node.input_refs.first() {
                Some(r) => input_width_v1(*r, table, profile).ok_or_else(over)?,
                None => 0,
            };
            // **ADR-0082 Decisions 1-3: is THIS node a fused attention site tried by dissection?**
            // Read off the node's kind and the RULESET's shape, never off a fence — see
            // `PalwCourtCostShapeV1::dissection`. A fused node under a court with no dissection
            // falls through to the whole-row arms below, which is design A's width and the honest
            // price of a court that cannot play the short protocol.
            let fused_dissection = if node.op_kind == Op::AttnFused { shape.dissection } else { None };
            // One head's dimension, and the lanes ONE dispute puts in question: a fused leaf is
            // disputed a head at a time (`PalwAttnRootClaimV1::lane_first/lane_count`), so a tile
            // wider than a head still disputes at most a head's lanes — which is also why the
            // dissection's wire cap is `attn_head_dim` and not `PALW_STEP_MAX_TILE_LEN`.
            let d_head = profile.attn_head_dim as u64;
            let disputed_lanes = tile.min(d_head.max(1));
            // The bottom's width: `PALW_ATTN_HISTORY_TILE_V4` positions, or the whole context when
            // the context is shorter than one tile. A CONSTANT past the tile — the property
            // Decision 2 buys and the one Z0 sweeps for.
            let history_tile = (crate::palw_state_chunk_map::PALW_ATTN_HISTORY_TILE_V4 as u64).min(n_ctx.max(1));

            // The artifact bytes this node's parameters occupy, per catalogued kernel. A node with
            // no weight operand opens nothing from the artifact — its inputs ride the leg.
            // ONE list, in the module that owns the slice derivations — this arm carried its own
            // copy for a round and priced a lane-sliced node as if it opened whole rows.
            let lane_sliced = crate::palw_step_refute::palw_kernel_is_lane_sliced_v1(node.kernel_semantics_id)
                || crate::palw_step_refute::palw_kernel_is_head_sliced_v1(node.kernel_semantics_id);
            let strided_combine =
                node.kernel_semantics_id == crate::palw_step::kernel_semantics_id_v1(crate::palw_step_refute::KDESC_Q36_MOE_COMBINE);
            let head_sliced_gdn = node.op_kind == Op::GatedDeltaNet
                && node.kernel_semantics_id == crate::palw_step::kernel_semantics_id_v1(crate::palw_step_refute::KDESC_Q36_GDN_STEP);
            let sliced_conv = node.op_kind == Op::SsmConv
                && node.kernel_semantics_id == crate::palw_step::kernel_semantics_id_v1(crate::palw_step_refute::KDESC_Q36_SSM_CONV);
            let opening = if node.weight_name.is_empty() {
                0
            } else {
                let attn_reduction = [crate::palw_step_refute::KDESC_A16_ATTN_SCORES, crate::palw_step_refute::KDESC_A16_ATTN_VALUES]
                    .iter()
                    .any(|d| crate::palw_step::kernel_semantics_id_v1(d) == node.kernel_semantics_id);
                let payload = match node.op_kind {
                    // The attention reductions multiply ACTIVATIONS (q·K, probs·V); their named
                    // "weight" is one narrowing triple, not a matrix, so charging `tile × in_w`
                    // priced an opening the executor never requests — 65 KiB of nothing at the
                    // scores step, which was the class's binding node for one whole round of the
                    // derivation.
                    Op::MatMulQuant if attn_reduction => 17,
                    // **The fused site opens the same narrowings the four nodes opened, once**
                    // (ADR-0082 Decision 1): W9's score triple, the probability requantization's
                    // and W10's value triple, plus the softmax's widening byte —
                    // `A16AttnFusedParamsV1`'s own fields, priced at the wire size the triple
                    // serialises at. Like the reductions above it multiplies ACTIVATIONS, so its
                    // named tensor is a narrowing record and not a matrix; charging `tile × in_w`
                    // here would price a matmul against the cache.
                    Op::AttnFused => 3 * crate::palw_base0_a16::A16QuantParams::WIRE_BYTES as u64 + 1,
                    // The head-sliced recurrence opens its head's four registered triples.
                    Op::GatedDeltaNet if head_sliced_gdn => 4 * 17,
                    // Tile-local since Decision B: the tile's own weight rows, one byte per int8.
                    // The GROUPED pair also opens its per-32 exponent rows and its per-row
                    // triples (the `.exp` / `.a16` suffix reads the executor performs beside the
                    // codes — for a routed node, per chosen expert at local offsets, which sums
                    // to the same tile volume). A block-misaligned routed geometry reads up to
                    // two covering chunks more per block boundary; the projector's own budgets
                    // keep every expressible geometry aligned, and the term is priced only when
                    // the alignment does not hold.
                    Op::MatMulQuant => {
                        let grouped = [
                            crate::palw_step_refute::KDESC_Q36_MATMUL_GROUPED,
                            crate::palw_step_refute::KDESC_Q36_MATMUL_GROUPED_WIDE,
                        ]
                        .iter()
                        .any(|d| crate::palw_step::kernel_semantics_id_v1(d) == node.kernel_semantics_id);
                        let codes = tile.checked_mul(in_w).ok_or_else(over)?;
                        if grouped {
                            let groups = in_w.div_ceil(crate::palw_qwen36_ops::QWEN36_WEIGHT_GROUP as u64);
                            let row_unit = in_w.checked_add(groups).and_then(|v| v.checked_add(17)).ok_or_else(over)?;
                            let mut payload = codes
                                .checked_add(tile.checked_mul(groups.checked_add(17).ok_or_else(over)?).ok_or_else(over)?)
                                .ok_or_else(over)?;
                            if node.weight_name.ends_with(".routed") {
                                let block_w = if node.weight_name.ends_with(".ffn_down_exps.routed") {
                                    profile.hidden_dim as u64
                                } else {
                                    profile.ffn_dim as u64
                                };
                                let chunk = tile.min(block_w.max(1));
                                if block_w > 0 && (!block_w.is_multiple_of(chunk) || (tile < block_w && !block_w.is_multiple_of(tile)))
                                {
                                    payload = payload
                                        .checked_add(2u64.checked_mul(chunk).and_then(|c| c.checked_mul(row_unit)).ok_or_else(over)?)
                                        .ok_or_else(over)?;
                                }
                            }
                            payload
                        } else {
                            codes
                        }
                    }
                    // (multiplier LE, shift, zero LE) per channel — per SLICE channel where the
                    // kernel is lane-sliced, because the executor reads its triples at the
                    // slice's own offset and nothing else is served.
                    Op::MulElem if lane_sliced || strided_combine => 9u64.checked_mul(tile.min(out_w)).ok_or_else(over)?,
                    // Four taps and one triple per SLICE channel.
                    Op::SsmConv if sliced_conv => 21u64.checked_mul(tile.min(out_w)).ok_or_else(over)?,
                    Op::MulElem => 9u64.checked_mul(out_w).ok_or_else(over)?,
                    // cos then sin, four bytes each, one pair per two lanes.
                    Op::RopeImrope => 8u64.checked_mul(out_w / 2).ok_or_else(over)?,
                    // One (multiplier, shift) for the whole node.
                    Op::Scale => 5,
                    // A gather opens the row it gathers; a norm's gain is one value per channel.
                    Op::EmbedLookup => out_w,
                    _ => 4u64.checked_mul(out_w).ok_or_else(over)?,
                };
                payload.checked_add(PALW_ARTIFACT_MAX_PATH_ELEMENTS * PATH_ELEMENT_BYTES).ok_or_else(over)?
            };

            // The challenged output tile: its own values, and the path that proves it.
            let leaf_bytes = |values: u64| values.checked_mul(4).and_then(|v| v.checked_add(step_path_bytes));
            let mut evidence = leaf_bytes(tile).ok_or_else(over)?;

            // And every leaf the court requires the challenger to open beside it. One row per
            // reference, except the KV arms, which are one row PER POSITION — that arm is the
            // reason this function had to grow past the weight bytes.
            //
            // **The un-anchored form, deliberately.** ADR-0030 §3's `PalwCheckpointKvOperandsV1`
            // replaces that history with one checkpoint opening and its state chunks, and it is
            // strictly cheaper — but it is not available at the coordinates that cost most. A
            // checkpoint covers a DECODE call, so a dispute at decode call `c` needs the one
            // covering `c − 1`: there is none for `c = 1`, and none at all for a prefill position.
            // The class's worst job is its whole context as prefill, so the worst step is exactly
            // where no anchor exists. Bounding the cheap form would have understated the shipped
            // floor by 2x, and `the_derived_close_cost_bounds_a_real_one` is what said so.
            // **How many POSITIONS this step opens its inputs at, from the node's own kernel.**
            //
            // `required_positions` expands a `GdnCore` step over every prior position — `0..=p`
            // inside the prefill call, and all of prefill plus every decode call at a decode step —
            // and each of those positions opens every one of the node's refs. Every other program
            // opens one position. The multiplier therefore belongs to the OP, not to the kind of
            // reference: a real GatedDeltaNet node wires its five inputs as ordinary intra-table
            // indices, so keying this off the checkpoint sentinel charged them once and priced the
            // recurrence at 6,592 bytes where the court requires 327,680 of Merkle path alone.
            // `the_gdn_arm_covers_what_the_court_actually_opens` is that measurement.
            // **The sliced kernels price what their court opens** — the same derivation
            // (`qwen36_gdn_slice_v1` / `qwen36_conv_slice_v1`) the leaf set and the executor
            // read, restated here as widths because the bound integrates over every tile while
            // those functions answer for one. A head-sliced recurrence still replays every
            // position, but each position opens one head's slices; the channel-sliced conv opens
            // four window positions of ONE ref's channel range (it was priced at a single
            // position before, which under-charged the window by 4x — found while the slicing
            // moved the width the other way).
            //
            // **`history` rather than `n_ctx` since ADR-0077 Decision 11**, and the two are the
            // same value on every shipped preset (`genesis_anchored_v1` sets `history_positions`
            // to `n_ctx`). Past the ladder fence a class with a registered state chunk map replays
            // at most `interval` positions after a verified checkpoint, which is the one term this
            // decision makes flat.
            let positions = match node.op_kind {
                Op::GatedDeltaNet => history,
                Op::SsmConv if sliced_conv => 4,
                _ => 1,
            };
            // Does this node read HISTORY at all? A KV arm or a recurrence does; nothing else. It
            // is what decides whether a checkpoint opening is paid for, so it is read off the same
            // node the runs below are.
            let reads_history = node.op_kind == Op::GatedDeltaNet
                || node.input_refs.iter().any(|r| *r == PALW_STEP_INPUT_KV_K || *r == PALW_STEP_INPUT_KV_V);
            // **A dissected fused site opens no ref's ROW.** Its three refs are the query, the K
            // cache and the V cache, and Decision 2's protocol never carries any of them whole:
            // what it carries is the bottom below. Emptied here rather than branched around the
            // whole loop so the two forms stay one walk — a second walk that merely agreed with
            // this one is how a class gets admitted at one price and prosecuted at another.
            let priced_refs: &[u16] = if fused_dissection.is_some() { &[] } else { node.input_refs.as_slice() };
            for (ordinal, r) in priced_refs.iter().enumerate() {
                let mut width = input_width_v1(*r, table, profile).ok_or_else(over)?;
                if head_sliced_gdn {
                    // Ref order is the kernel's: [unit_k, conv, unit_q, decay, beta] — one head's
                    // slice of each.
                    width = match ordinal {
                        0 | 2 => profile.gdn_head_k_dim as u64,
                        1 => profile.gdn_head_v_dim as u64,
                        _ => 1,
                    };
                }
                if sliced_conv {
                    if ordinal != 0 {
                        // One ref per position carries the challenged channels; the others open
                        // nothing. Priced as ref 0 because the three regions share a tile width
                        // and the bound wants the shape, not the identity.
                        continue;
                    }
                    width = tile;
                }
                if lane_sliced {
                    // Every ref opens the challenged tile's own lane range.
                    width = tile.min(width);
                }
                if strided_combine && ordinal == 0 {
                    // The outputs ref: `k` expert blocks, each contributing the tile's lanes.
                    // `k` is the blocks in the concatenation — outputs width over the node's row.
                    let k = width / out_w.max(1);
                    width = k.max(1).checked_mul(tile).ok_or_else(over)?;
                }
                let src_tile = source_tile_len_v1(table, node, *r);
                // **Priced as RANGE RUNS, because that is the carrier's form.** One position of
                // one ref is a contiguous run of tiles (the enumeration puts a node's tiles at
                // consecutive indices), and a run costs its lanes once plus ONE bounded sibling
                // set — `depth + ceil(log2 k) + 1` hashes, the range walk's own worst case —
                // plus a small per-leaf preimage header. Per-leaf full paths priced a 2,048-lane
                // row at tile 8 at 327 KiB of path for 8 KiB of lanes, and the whole class's
                // admissibility hung on exactly that difference.
                let (runs, run_tiles, run_lanes) = match *r {
                    // The cache: one run per position, each `kv_dim` wide.
                    PALW_STEP_INPUT_KV_K | PALW_STEP_INPUT_KV_V => (history, kv_dim.div_ceil(src_tile), kv_dim),
                    // Everything else: `positions` runs of the (possibly sliced) row. The
                    // checkpoint sentinel stays priced pessimistically on purpose: the leaf set
                    // refuses it outright today, so a class reaching it is unadjudicable rather
                    // than cheap, and a low price here would read as approval.
                    _ => (positions, width.div_ceil(src_tile), width),
                };
                let run_path = step_path_bytes
                    .checked_add(64 * (u64::from(run_tiles.max(1).next_power_of_two().trailing_zeros()) + 1))
                    .ok_or_else(over)?;
                let per_run = run_lanes
                    .checked_mul(4)
                    .and_then(|lanes| lanes.checked_add(run_path))
                    .and_then(|v| v.checked_add(24u64.checked_mul(run_tiles)?))
                    .ok_or_else(over)?;
                evidence = evidence.checked_add(runs.checked_mul(per_run).ok_or_else(over)?).ok_or_else(over)?;
            }

            // **The bottom of the dissection, and one move** (ADR-0082 Decision 2 and §4).
            //
            // A fused attention leaf is not recomputed whole: the terminal adjudication opens the
            // head's query slice, ONE tile of K rows and ONE tile of V rows, and recomputes the
            // tile's `(max, exp_sum, v_acc)` against the root's `(m*, S*)`. The committed output
            // tile and its path are already `evidence`'s seed, so this adds the three openings the
            // bottom puts beside it — each its own lanes plus one Merkle path, the same units
            // every other opening on this close is counted in. Every term is a MODEL width times a
            // CONSTANT tile, so the whole of it is flat in `n_ctx` (Z0).
            //
            // Plus the widest disclosure ONE round carries. A move rides a carrier exactly as a
            // close does, and an arity whose round no carrier holds is not a shorter court but an
            // unplayable one — `palw_attn_dissect_arity_fits_carrier_v1` states the bound and the
            // arity derivation applies it; charging it here is what makes the CLASS's admission
            // depend on it rather than the court's configuration alone.
            if let Some(arity) = fused_dissection {
                // The cache-write route opens each row at the CACHE-WRITE node's tile, not at this
                // node's — the same rule `source_tile_len_v1` states for every other opened run.
                let source_tile = source_tile_len_v1(table, node, PALW_STEP_INPUT_KV_K);
                let bottom =
                    palw_attn_bottom_bytes_v1(d_head, kv_dim, history_tile, tile, source_tile, step_path_bytes).ok_or_else(over)?;
                let move_bytes = crate::palw_attn_dissect::palw_attn_dissect_move_bytes_v1(arity, disputed_lanes as usize);
                // The committed output tile is inside `bottom` (both routes carry it), so the
                // generic seed this arm was added after would double-count it.
                evidence = evidence
                    .checked_sub(leaf_bytes(tile).ok_or_else(over)?)
                    .and_then(|e| e.checked_add(bottom))
                    .and_then(|e| e.checked_add(move_bytes))
                    .ok_or_else(over)?;
            }

            // **The routing appendix.** A routed reader's canonical set appends the layer's
            // committed `RouterTopk` row after the declared refs — one more short run, priced
            // exactly as the leaf derivation opens it.
            if crate::palw_step_refute::qwen36_reads_routing_v1(node) {
                let topk = crate::palw_step::kernel_semantics_id_v1(crate::palw_step_refute::KDESC_Q36_ROUTER_TOPK);
                if let Some(router) = table.iter().find(|n| n.kernel_semantics_id == topk) {
                    let lanes = node_out_width_v1(router, profile).ok_or_else(over)?;
                    let tiles = lanes.div_ceil((router.tile_len as u64).max(1));
                    let run_path = step_path_bytes
                        .checked_add(64 * (u64::from(tiles.max(1).next_power_of_two().trailing_zeros()) + 1))
                        .ok_or_else(over)?;
                    let appendix = lanes
                        .checked_mul(4)
                        .and_then(|l| l.checked_add(run_path))
                        .and_then(|v| v.checked_add(24u64.checked_mul(tiles)?))
                        .ok_or_else(over)?;
                    evidence = evidence.checked_add(appendix).ok_or_else(over)?;
                }
            }

            // **The generated-token pin** (ADR-0049 Decision E). A gather at a DECODE position
            // cannot be adjudicated without the ids the model produced, and the integer lane pins
            // them by carrying every logits row so the court can recompute
            // `base0_logits_trace_root_v1` — a flat hash, not a tree, so one row cannot be opened
            // on its own. That makes this arm `calls x vocabulary`, and a job may be almost all
            // decode: the bound is the whole context. Only a gather pays it.
            if shape.count_ids && node.op_kind == Op::EmbedLookup {
                let ids = n_ctx.checked_mul(4).ok_or_else(over)?;
                let pin = decode_pin_price_v1(profile, n_ctx).ok_or_else(over)?;
                evidence = evidence.checked_add(ids).and_then(|e| e.checked_add(pin)).ok_or_else(over)?;
            }
            // The prompt ids ride every refutation that addresses a gather, and a challenger may
            // carry them on any close: they are checked against `prompt_token_ids_hash` before one
            // is read, so they cost bytes rather than trust.
            //
            // **Not anchored by ADR-0077 Decision 11, and ADR-0081 Decision 3 is why it could not
            // be.** Decision 11 shortens the history a ref opens; it says nothing about the ids,
            // because under the FLAT `prompt_token_ids_hash` no window of ids can be opened
            // against the commitment at all — so the whole prompt rides every close and the term
            // is `n_ctx × 4`, which is what ADR-0077 §4 budgets ("PublicDa carries `n_ctx × 4`
            // bytes of ids — 2 KiB at 512"). It is the reason W1's `max_close_bytes` equality is
            // stated over the history term rather than over the whole close.
            //
            // Past `Params::palw_prompt_ids_merkle` the commitment is a tiled Merkle root, a
            // refutation carries ONE tile and its path, and this term becomes that opening's size
            // — 472 bytes at `n_ctx` 512 against 2,048, and 856 at 32,768 against 131,072, which
            // alone is past the whole carrier. `prompt_ids_close_bytes_v1` is the one derivation:
            // the price a class is admitted at has to be the price its challengers pay, and a
            // bound that guessed here would drift from the carrier the moment either moved.
            if shape.count_ids {
                let ids = crate::palw_prompt_ids_v1::prompt_ids_close_bytes_v1(shape.prompt_ids_form, n_ctx).ok_or_else(over)?;
                evidence = evidence.checked_add(ids).ok_or_else(over)?;
            }
            // **The price of stopping at a checkpoint** (Decision 11: "plus ONE checkpoint-chunk
            // opening per history-reading ref"). Zero on the shipped form, which has no anchor to
            // open.
            //
            // **Per REF for the cache and per NODE for the recurrence, because that is how many
            // OBJECTS each one substitutes.** A KV arm's two sentinels address two different
            // committed series (`K` and `V`) and each stands on its own chunk. A recurrence's five
            // refs are five slices of ONE state — the delta matrix and the convolution window that
            // `gdn_state_row_bytes_for_map_v1` prices together — so charging five of them billed a
            // Qwen3.6 row 358,400 bytes for an object that opens once at 71,680. The rule is
            // "one opening per anchored OBJECT", and the recurrence's five refs are one object.
            if reads_history {
                let charge = if node.op_kind == Op::GatedDeltaNet {
                    shape.gdn_checkpoint_bytes
                } else if node.op_kind == Op::AttnFused {
                    // TWO anchored objects, counted by name rather than by ref count: the K series
                    // and the V series each stand on their own checkpoint chunk, and the query ref
                    // is a committed step row with no checkpoint to open. `refs.len()` would have
                    // billed a fused site three cache openings for two caches.
                    let series =
                        node.input_refs.iter().filter(|r| **r == PALW_STEP_INPUT_KV_K || **r == PALW_STEP_INPUT_KV_V).count() as u64;
                    series.checked_mul(shape.kv_checkpoint_bytes).ok_or_else(over)?
                } else {
                    let refs = node.input_refs.len().max(1) as u64;
                    refs.checked_mul(shape.kv_checkpoint_bytes).ok_or_else(over)?
                };
                evidence = evidence.checked_add(charge).ok_or_else(over)?;
            }

            // Recomputation: what a full node spends to redo this one step, on peer-supplied input.
            //
            // The recurrence is its own case for the same reason it is in the byte arm, and it was
            // wrong the same way. `gdn_core_genesis_replay` walks EVERY prior position and, per
            // position and head, makes a small constant number of passes over a `k_dim x v_dim`
            // state: decay it, take `v_dim` dot products of length `k_dim` against it, then update
            // it. Charging `out_w` — the width of the row it finally emits — priced the fixture's
            // recurrence at 32 against 32,768 actually performed, and a Qwen3.6-shaped one at 2,048
            // against ~67 M, which is four times the ceiling it would have been admitted under.
            // Four passes is above the three the kernel makes, so this is a bound rather than an
            // estimate.
            let macs = match node.op_kind {
                Op::MatMulQuant => tile.checked_mul(in_w).ok_or_else(over)?,
                // **The bottom recomputes ONE tile** (ADR-0082 Decision 2): `history_tile` scores,
                // each a dot of `d_head`, and `history_tile` weighted contributions per disputed
                // lane. The exponent and the probability are per element given the root's
                // `(m*, S*)` — one table lookup and one multiply-shift each — which is exactly
                // what makes the tile recomputable without the row, so they ride the same count.
                Op::AttnFused if fused_dissection.is_some() => history_tile
                    .checked_mul(d_head.checked_add(disputed_lanes).ok_or_else(over)?)
                    .ok_or_else(over)?,
                // Without the dissection the court has no bottom to stand on and recomputes the
                // whole row: the history's scores and the history's weighted sum.
                Op::AttnFused => {
                    history.checked_mul(d_head.checked_add(disputed_lanes).ok_or_else(over)?).ok_or_else(over)?
                }
                // The head-sliced form divides the recomputation by the head count: the court
                // replays ONE head's `k_dim x v_dim` state, which is what lets a 40-layer hybrid
                // have a context at all (the whole-graph form priced 536 M at the declared
                // context, 32x the terminal ceiling — measured, not estimated, on the first
                // profile to reach this arm).
                Op::GatedDeltaNet if head_sliced_gdn => positions
                    .checked_mul(profile.gdn_head_k_dim as u64)
                    .and_then(|v| v.checked_mul(profile.gdn_head_v_dim as u64))
                    .and_then(|v| v.checked_mul(4))
                    .ok_or_else(over)?,
                Op::GatedDeltaNet => positions
                    .checked_mul(profile.gdn_heads as u64)
                    .and_then(|v| v.checked_mul(profile.gdn_head_k_dim as u64))
                    .and_then(|v| v.checked_mul(profile.gdn_head_v_dim as u64))
                    .and_then(|v| v.checked_mul(4))
                    .ok_or_else(over)?,
                _ => out_w,
            };

            let operands = node.input_refs.len() as u32 + u32::from(!node.weight_name.is_empty());
            let close = opening.checked_add(evidence).ok_or_else(over)?;
            if let Some(rows) = sink.as_mut() {
                rows.push(PalwCourtCostRowV1 {
                    table: table_name,
                    index: node_index,
                    op_kind: node.op_kind,
                    weight_name: node.weight_name.clone(),
                    opening_bytes: opening,
                    evidence_bytes: evidence,
                    close_bytes: close,
                    terminal_macs: macs,
                });
            }
            cost.max_close_bytes = cost.max_close_bytes.max(close);
            cost.max_terminal_macs = cost.max_terminal_macs.max(macs);
            cost.max_operand_count = cost.max_operand_count.max(operands);
        }
    }
    Ok(cost)
}

/// Why a class may not join.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwClassAdmissionError {
    #[error("the object is not a class registration")]
    NotARegistration,
    /// A class IS its graph. Two registrations of the same profile are the same class and a
    /// different profile cannot borrow an id, so the id is checked against the profile rather than
    /// accepted as a label.
    #[error("the declared class id is not this profile's id")]
    ClassIdIsNotTheProfileId { declared: Hash64, derived: Hash64 },
    #[error("the profile is not well-formed: {0}")]
    Profile(String),
    /// ADR-0038 A4. Every kernel the graph can reach must be one this build can adjudicate,
    /// or every dispute over the uncatalogued one ends `Unadjudicable` — rejected but unslashed.
    #[error("the class reaches kernels this build cannot adjudicate")]
    CoverageGap,
    /// The class's LONGEST job — its whole context as prefill — must fit the ladder the ruleset
    /// already froze. Checking the typical job instead would admit a class an attacker picks the
    /// job length for.
    #[error("the class's worst-case trace is deeper than the court's ladder: {worst} > {ladder}")]
    DeeperThanTheLadder { worst: u64, ladder: u64 },
    /// ADR-0049 Decision C. A class whose cheapest prosecutable step still costs more than the
    /// ruleset allows is a class whose disputes nobody can raise — coverage-clean, ladder-deep
    /// enough, and unpolicable.
    #[error("the class's {what} of {got} exceeds the ruleset's ceiling of {ceiling}")]
    CourtCostExceedsCeiling { what: &'static str, got: u64, ceiling: u64 },
    /// A network that carries value registers only derived classes (the genesis loader's rule,
    /// restated: `MaxPerAttempt` bounds rather than checks, which makes PALW weight a collateral
    /// measure instead of a work measure).
    #[error("the class is not a derived-pwu class")]
    ClassIsNotDerived,
    #[error("the declared pwu_per_inference is not the canonical job's counted leaves: {declared} != {counted}")]
    PwuPerInferenceMismatch { declared: u64, counted: u64 },
    /// The canonical job is what the class is PAID per, so it may not be longer than the worst
    /// case the ladder was checked against.
    #[error("the canonical job is deeper than the class's own worst case: {canonical} > {worst}")]
    CanonicalDeeperThanWorstCase { canonical: u64, worst: u64 },
    /// **ADR-0069 Decision 5: weight is the thing certification buys.**
    ///
    /// Static adjudicability (every check above this one) says the court could re-execute this
    /// graph's steps. It does not say any backend can actually PLAY the dispute — assemble a
    /// refutation, answer a rung, close — and the launch audit measured exactly that gap: two
    /// families holding 97.8% of cadence whose court methods are the trait's defaults. A class
    /// that fails here is still perfectly registrable; it takes a zero share and earns no cadence
    /// until some build certifies a backend for it.
    #[error(
        "the class asks for {share}‰ but no end-to-end certified family covers the kernels it reaches — it may register weightless"
    )]
    NotEndToEndCertified { share: u16 },
    /// **ADR-0077 Decision 14: the canonical job grows with the context.**
    ///
    /// A quantum is `pwu_per_inference / quanta_per_canonical_job` leaves and a receipt is capped
    /// at `max_quanta_per_receipt` of them — a per-receipt JACKPOT bound (ADR-0044 Decision 5),
    /// not a tax on ordinary use. A row whose canonical job is a fraction of its context makes it
    /// the second thing: with the hybrid's (7, 2) job at `n_ctx` 512 an ordinary request is ~450
    /// quanta capped to 64, so 86 % of real, certified work is uncounted. Requiring the canonical
    /// footprint to be at least an eighth of the context bounds the widest admissible job at
    /// `8 × 8 = 64` quanta by construction, and the cap goes back to bounding the outlier.
    ///
    /// Reachable only past `Params::palw_context_ladder` — see
    /// [`crate::palw_context_ladder::palw_canonical_footprint_floor_v1`].
    #[error("the canonical job touches {footprint} cached positions and this row's floor is {floor}")]
    CanonicalFootprintUnderTheRow { footprint: u64, floor: u64 },
    /// **ADR-0082 Decision 1, under ADR-0049 Decision C's rule: a court that cannot try a leaf
    /// must not admit the class that produces one.**
    ///
    /// An `AttnFused` node's terminal adjudication is not a recompute — it is the history
    /// dissection (ADR-0082 Decision 2), and `palw_step_refute`'s execution arm says so by
    /// returning `Unadjudicable` until U-03 lands the court's side. A ruleset whose
    /// `Params::palw_kary_court` is unset has no dissection at all, so every dispute over such a
    /// class ends rejected-but-unslashed: unfalsifiable work on a chain where bonds are supposed
    /// to be at risk, which is the exact failure the coverage gates exist to refuse. Refused BY
    /// NAME rather than through the coverage gate because the graph is perfectly catalogued — what
    /// is missing is the COURT, and a refusal that said "coverage gap" would send the reader to
    /// the adjudicator instead of to the fence.
    #[error("the class carries a fused attention site and this ruleset's court has no dissection to try it with")]
    FusedAttentionNeedsTheKaryCourt,
    /// **The price a class is admitted at has to be the price its challengers pay.**
    ///
    /// The cost shape is assembled by `palw_class_ladder_rules_for_court_v1` from the caller's
    /// reading of the ruleset, and the arity inside it must be the one the ruleset froze
    /// (`PalwCourtParamsV2::dissection_arity`). A caller that armed the fence but built the rules
    /// without the dissection would admit a fused row at the WHOLE-ROW price — safe, since that is
    /// larger — and one that built them at a wider arity than the court plays would admit it at a
    /// move nobody can make. Neither is allowed to be silent.
    #[error("the class is priced for a dissection of {priced:?} and this ruleset's court plays {court}")]
    PricedForADifferentCourt { priced: Option<u8>, court: u8 },
    /// **ADR-0082 Decision 3 and Z4/Z11: the whole dispute has to fit `window_court`.**
    ///
    /// `(2 x (ladder rounds + history rounds) + terminal) x turn_deadline`, with every term read
    /// from the ruleset. The third of the three bounds a graph-v5 row must satisfy at once, and
    /// the one neither the close nor the ladder can see: a row can be cheap to carry and shallow
    /// enough to enumerate and still take more DAA to prosecute than the lattice leaves for it.
    #[error("prosecuting the class's widest row takes {needed} DAA and this lattice's court window is {window}")]
    CourtWindowTooShort { needed: u64, window: u64 },
}

/// **The Phase B rules a `palw_context_ladder`-armed network judges a registration under**
/// (ADR-0077 Decisions 11, 12 and 14).
///
/// Assembled by [`crate::palw_context_ladder::palw_class_ladder_rules_v1`] from the profile's own
/// map and context, so nothing here is a registrant's declaration. `None` at
/// [`verify_class_admission_v4`] is the shipped gate, byte for byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwClassLadderRulesV1 {
    /// The ladder both leaf counts are enumerated against.
    pub ladder: u64,
    /// The court this class's close is priced for.
    pub cost_shape: PalwCourtCostShapeV1,
    /// Decision 14's floor under the canonical job's footprint.
    pub canonical_footprint_floor: u64,
}

/// **What one opening's FRAME costs on the wire**, derived from the two structs an opening is
/// (`PalwStepTileLeafV1` and `PalwStepOpeningV1`) rather than measured off an object.
///
/// `PalwStepTileLeafV1`: version `u16` (2), the four `u32`s of `PalwStepCoordinateV1` (16),
/// `value_count` (4) and the values vector's length prefix (4) — 26. `PalwStepOpeningV1`:
/// `leaf_index` `u64` (8), `leaf_hash` `Hash64` (64) and the siblings vector's length prefix (4) —
/// 76. The payload and the path are counted separately by every caller, exactly as
/// `arithmetic_close_bytes_v2` counts them.
pub const PALW_STEP_OPENING_FRAME_BYTES: u64 = 26 + 76;

/// The `PalwAttnDissectBottomV1` envelope, field by field: `version` `u16` (2), `session_id`
/// `Hash64` (64), `tile` `u64` (8), the `anchor: Option<_>` tag (1), and for each of `k` and `v`
/// the `PalwAttnTileEvidenceV1` discriminant (1) beside its inner vector's length prefix (4).
///
/// Derived from the struct rather than measured off an object, and then CHECKED against one:
/// `the_derived_bottom_bounds_the_real_bottom_object` builds the real wire type at both registered
/// head widths and `kv_heads` 2 and asserts the derivation is not below borsh's answer. It was
/// three bytes below before that test existed — the two enum tags and the option's — which is
/// exactly the direction a cost bound may not be wrong in.
pub const PALW_ATTN_BOTTOM_ENVELOPE_BYTES: u64 = 2 + 64 + 8 + 1 + 2 * (1 + 4);

/// **Route A — the bottom reached through the CHECKPOINT TILE** (ADR-0082 Decision 4).
///
/// The head's query slice, one K tile and one V tile as SINGLE openings against the class's
/// registered state chunk map, and the committed output tile: four openings, four paths, flat in
/// `n_ctx`. This is the route Decision 4 exists to make available and the one ADR-0082 §4's
/// "~25 KB on the dense tier, ~42 KB on the hybrid" prices.
///
/// It is not the route the court can play today — `PalwAttnDissectBottomV1` carries cache-write
/// leaves, and `state_chunk_opening_root_v1` is not landed — so a cost that charged only this
/// would be smaller than the object a challenger actually files, which is the defect
/// `the_derived_close_cost_bounds_a_real_one` exists to refuse. [`palw_attn_bottom_bytes_v1`]
/// takes the larger of the two.
pub fn palw_attn_bottom_tile_route_bytes_v1(
    d_head: u64,
    kv_dim: u64,
    tile_positions: u64,
    out_lanes: u64,
    step_path_bytes: u64,
) -> Option<u64> {
    let opening = |lanes: u64| -> Option<u64> {
        lanes.checked_mul(4)?.checked_add(step_path_bytes)?.checked_add(PALW_STEP_OPENING_FRAME_BYTES)
    };
    // **`kv_dim`, not `d_head`.** ADR-0082 §4 sizes this term as `2 x 16 x 4 x d_head` — one
    // HEAD's slice — and a checkpoint chunk cannot be narrowed to a head: the map addresses
    // `(kind, layer, position)` and a chunk holds the whole cache ROW
    // (`palw_attn_court_v1`'s own assertion, `chunk_bytes.len() == TILE x kv_dim x 4`). On both
    // registered families `attn_kv_heads` is 2, so the term is twice the ADR's.
    let tile_lanes = tile_positions.checked_mul(kv_dim)?;
    opening(d_head)?
        .checked_add(opening(tile_lanes)?)?
        .checked_add(opening(tile_lanes)?)?
        .checked_add(opening(out_lanes)?)?
        .checked_add(PALW_ATTN_BOTTOM_ENVELOPE_BYTES)
}

/// **Route B — the bottom reached through the CACHE-WRITE LEAVES**, which is what the court plays
/// today (`palw_attn_court_v1::PalwAttnDissectBottomV1`).
///
/// One opening per committed TILE of every row it carries: the query row, the tile's K rows and V
/// rows, and the output tile. That is the difference that matters and the reason this is derived
/// rather than copied from the object's measured size — a cache row is committed at the CLASS's
/// `tile_len`, so opening one row's `d_head` lanes is `⌈d_head / tile_len⌉` leaves and each one
/// carries its own full Merkle path. At the shipped 8-lane dense tile that is sixteen paths a row
/// where the tile route pays one.
pub fn palw_attn_bottom_cache_write_bytes_v1(
    d_head: u64,
    kv_dim: u64,
    tile_positions: u64,
    out_lanes: u64,
    source_tile: u64,
    step_path_bytes: u64,
) -> Option<u64> {
    let per_leaf = step_path_bytes.checked_add(PALW_STEP_OPENING_FRAME_BYTES)?;
    let row = |lanes: u64| -> Option<u64> {
        let leaves = lanes.div_ceil(source_tile.max(1)).max(1);
        leaves.checked_mul(per_leaf)?.checked_add(lanes.checked_mul(4)?)
    };
    // The QUERY is the head's slice of a committed row; a CACHE row is the whole `kv_dim`, for the
    // same reason the chunk is — the cache-write node commits `attn_kv_heads x attn_head_dim`
    // lanes and a leaf of it is a tile of that row, not of one head's share.
    let cache_rows = tile_positions.checked_mul(2)?;
    cache_rows
        .checked_mul(row(kv_dim)?)?
        .checked_add(row(d_head)?)?
        .checked_add(row(out_lanes)?)?
        .checked_add(PALW_ATTN_BOTTOM_ENVELOPE_BYTES)
}

/// **What a fused leaf's bottom costs: the LARGER of the two routes** (ADR-0082 Z3).
///
/// A challenger picks the route it can file, and the court must be able to carry whichever that
/// is; a derivation that priced only the cheaper one would admit a class at a price its disputes
/// cannot be brought at. When `state_chunk_opening_root_v1` lands, route A becomes playable and
/// this is where the choice stops being a max and becomes the ruleset's.
pub fn palw_attn_bottom_bytes_v1(
    d_head: u64,
    kv_dim: u64,
    tile_positions: u64,
    out_lanes: u64,
    source_tile: u64,
    step_path_bytes: u64,
) -> Option<u64> {
    let a = palw_attn_bottom_tile_route_bytes_v1(d_head, kv_dim, tile_positions, out_lanes, step_path_bytes)?;
    let b = palw_attn_bottom_cache_write_bytes_v1(d_head, kv_dim, tile_positions, out_lanes, source_tile, step_path_bytes)?;
    Some(a.max(b))
}

/// **Does this profile carry a fused attention site?** (ADR-0082 Decision 1.)
///
/// Read off the node kinds, which is the only place it can be read from: a class IS its graph
/// (ADR-0049 Decision F), so "is this graph v5" is not a flag a registration carries and not a
/// version number in the id — it is whether any table holds a [`crate::palw_step::PalwStepOpKindV1::AttnFused`]
/// node. One spelling, here, because three consumers ask it: the cost walk (which route prices the
/// site), the ladder rule (which interval anchors it) and the admission gate (whether a court that
/// cannot try the leaf may admit the class at all).
pub fn palw_profile_has_fused_attention_v1(profile: &PalwShapeProfileV3) -> bool {
    [&profile.pre_nodes, &profile.gdn_nodes, &profile.attn_nodes, &profile.post_nodes]
        .into_iter()
        .flatten()
        .any(|node| node.op_kind == crate::palw_step::PalwStepOpKindV1::AttnFused)
}

/// Every kernel a profile's graph can reach, read off the graph.
///
/// Public because the coverage claim and the catalog entry must be built from the same traversal —
/// two traversals that merely happen to agree is how A4 certifies a set nobody derived.
pub fn reachable_kernels_v1(profile: &PalwShapeProfileV3) -> BTreeSet<Hash64> {
    [&profile.pre_nodes, &profile.gdn_nodes, &profile.attn_nodes, &profile.post_nodes]
        .into_iter()
        .flatten()
        .map(|node| node.kernel_semantics_id)
        .collect()
}

/// The gate: `Ok(entry)` iff this class may join a chain running `bundle`.
///
/// `canonical` is the job the class is paid per. It is an argument rather than a derivation
/// because no function can choose it — it is the registrant's declaration of what one unit of this
/// class's work is — and a carrier must therefore commit it inside the signed registration, beside
/// `artifact_root`. Everything the catalog entry needs BESIDES that is computed here.
///
/// The returned entry is what a genesis catalog would have held for this class. A caller that
/// keeps it has the same object the genesis path produces, so the two lanes cannot drift into
/// describing a class differently.
///
/// `certified` is this build's end-to-end certified family set (ADR-0069 Decision 5), and it must
/// hash to the `court_e2e_root` `bundle` commits to — see [`crate::palw_e2e_adjudicability`]. It is
/// only read when the registration asks for a nonzero share: a weightless registration is admitted
/// on the static properties alone, which is ADR-0039's "admissible for liveness" and the reason
/// this gate can be strict about weight without being a gatekeeper on existence.
pub fn verify_class_admission_v2(
    bundle: &PalwConsensusParamsV2,
    profile: &PalwShapeProfileV3,
    canonical: &PalwJobContextV2,
    registration: &PalwConsensusObjectV2,
    certified: &[crate::palw_e2e_adjudicability::PalwE2eFamilyV1],
) -> Result<PalwClassCatalogEntryV2, PalwClassAdmissionError> {
    verify_class_admission_v3(bundle, profile, canonical, registration, certified, &[])
}

/// [`verify_class_admission_v2`] with the chain's own certified families in scope (ADR-0075
/// Decision 4): `chain_certified` is what `PalwChainStateV2::chain_certified_families(Attempt)`
/// returns at the block the registration is judged in, and a family there covers a class exactly
/// as a genesis one does. The v2 form is the same gate with that set empty.
pub fn verify_class_admission_v3(
    bundle: &PalwConsensusParamsV2,
    profile: &PalwShapeProfileV3,
    canonical: &PalwJobContextV2,
    registration: &PalwConsensusObjectV2,
    certified: &[crate::palw_e2e_adjudicability::PalwE2eFamilyV1],
    chain_certified: &[crate::palw_e2e_adjudicability::PalwE2eFamilyV1],
) -> Result<PalwClassCatalogEntryV2, PalwClassAdmissionError> {
    verify_class_admission_v4(bundle, profile, canonical, registration, certified, chain_certified, None)
}

/// [`verify_class_admission_v3`] under ADR-0077 Phase B's rules, or under today's.
///
/// `ladder` is `None` on every shipped preset — `Params::palw_context_ladder` is the fence, and
/// with it unset this is `verify_class_admission_v3` and derives the same catalog entry byte for
/// byte. `Some` swaps three derivations and adds one refusal, and every one of them is a
/// `PalwConsensusParamsV2`-shaped fact and therefore a re-mint: the ladder both leaf counts are
/// enumerated against (Decision 12), the court the close is priced for (Decision 11), and
/// Decision 14's floor under the canonical job.
#[allow(clippy::too_many_arguments)]
pub fn verify_class_admission_v4(
    bundle: &PalwConsensusParamsV2,
    profile: &PalwShapeProfileV3,
    canonical: &PalwJobContextV2,
    registration: &PalwConsensusObjectV2,
    certified: &[crate::palw_e2e_adjudicability::PalwE2eFamilyV1],
    chain_certified: &[crate::palw_e2e_adjudicability::PalwE2eFamilyV1],
    ladder: Option<PalwClassLadderRulesV1>,
) -> Result<PalwClassCatalogEntryV2, PalwClassAdmissionError> {
    verify_class_admission_v5(bundle, profile, canonical, registration, certified, chain_certified, ladder, None)
}

/// [`verify_class_admission_v4`] under a `palw_kary_court`-armed ruleset (ADR-0082 Decisions 1-6,
/// Z10 and Z11).
///
/// `court` is `None` on every shipped preset — `Params::palw_kary_court` is the fence, and with it
/// unset this is `verify_class_admission_v4` and derives the same catalog entry byte for byte,
/// with ONE addition that is a refusal rather than a price: a profile carrying `AttnFused` is
/// refused by name, because a court with no dissection cannot try that leaf and a class whose
/// every dispute ends `Unadjudicable` must not be admitted (ADR-0049 Decision C).
///
/// `Some` says the fence is armed and carries what the ruleset armed it with — the caller reads
/// `params.palw_kary_court_active_at(daa)`, `PalwCourtParamsV2::dissection_arity` and
/// `Params::palw_prompt_ids_form_at`, and passes the same [`PalwKaryCourtV1`] to
/// `palw_class_ladder_rules_for_court_v1` so the shape this gate prices with and the shape it
/// checks against are one object. Past it a graph-v5 row must satisfy **all three bounds at once**
/// and the refusal names which one refused:
///
/// * **the close** — `palw_close_chunks_for_bytes_v1(max_close_bytes) <= max_close_chunks`
///   (Decision 6), reported as `CourtCostExceedsCeiling { what: "court close chunks" }`;
/// * **the ladder** — the class's worst case under the ruleset's `max_step_leaf_count`
///   (Decision 1: with the fused node the count is the BASE count), reported as
///   `DeeperThanTheLadder`;
/// * **the window** — `worst_case_duration_with_history_daa` inside the lattice's `window_court`
///   (Decision 3), reported as [`PalwClassAdmissionError::CourtWindowTooShort`].
///
/// **The seat's window is NOT here.** ADR-0082 Decision 9 bounds what one seat can verify inside
/// `window_receipt x rate`, and Decision 9 says the CERTIFICATION DRILL enforces it — a property
/// of a build's measured throughput, which admission cannot read off a graph and must not pretend
/// to.
#[allow(clippy::too_many_arguments)]
pub fn verify_class_admission_v5(
    bundle: &PalwConsensusParamsV2,
    profile: &PalwShapeProfileV3,
    canonical: &PalwJobContextV2,
    registration: &PalwConsensusObjectV2,
    certified: &[crate::palw_e2e_adjudicability::PalwE2eFamilyV1],
    chain_certified: &[crate::palw_e2e_adjudicability::PalwE2eFamilyV1],
    ladder: Option<PalwClassLadderRulesV1>,
    court: Option<PalwKaryCourtV1>,
) -> Result<PalwClassCatalogEntryV2, PalwClassAdmissionError> {
    let PalwConsensusObjectV2::ClassRegistered { class_id, artifact_root, pwu_rule, share_permille, .. } = registration else {
        return Err(PalwClassAdmissionError::NotARegistration);
    };

    // **Every class that reaches this gate is court-adjudicable, and there is no arm that skips
    // it** (ADR-0053). The withdrawn Metal/GGUF family was admitted by a sibling arm that ran
    // `validate_geometry` instead of `validate_shape`, skipped the A4 coverage gate, skipped the
    // ladder-depth check and the court-cost ceilings, and wrote a catalog entry whose
    // `reachable_kernels` was EMPTY — a class the court could not adjudicate, admitted by a gate
    // whose whole job is refusing those. It is gone, and with it the question "which family is
    // this?" that every consumer downstream had to get right.

    // **Well-formedness first, before anything reads the shape.** Every check below — the id
    // derivation, the kernel walk, the leaf enumeration — is driven by `n_ctx` and `layer_count`,
    // so an unbounded shape decides how much work it costs to reject it. `validate_shape` is where
    // those bounds live; running it after the first consumer is running it too late.
    profile.validate_shape().map_err(|e| PalwClassAdmissionError::Profile(e.to_string()))?;

    let derived_id = profile.shape_profile_id();
    if *class_id != derived_id {
        return Err(PalwClassAdmissionError::ClassIdIsNotTheProfileId { declared: *class_id, derived: derived_id });
    }

    // **The logits scheme must be one THIS BUILD adjudicates and prices.** `validate_shape` only
    // required the class to state one — well-formedness — but a scheme the close arms cannot
    // dispatch is a class whose every decode-token dispute ends unadjudicated, which is the same
    // fail-open A4 refuses for kernels. Enumerated here rather than in `validate_shape` because
    // "which schemes exist" is a property of the adjudicator, exactly like the kernel catalog.
    let known_schemes = [crate::palw_step_refute::flat_logits_scheme_id_v1(), crate::palw_step_refute::tiled_logits_scheme_id_v1()];
    if !known_schemes.contains(&profile.logits_scheme_id) {
        return Err(PalwClassAdmissionError::Profile(format!(
            "the class commits its logits under scheme {}, which this build cannot adjudicate",
            profile.logits_scheme_id
        )));
    }

    // **The canonical job must fit the context the class registered — in the ENUMERATION's own
    // form.** The step space's largest cached-position count is `prefill + exact_decode − 1`
    // (`step_leaf_count`: prefill runs kv_len = p+1 for p < prefill, decode call c runs
    // kv_len = prefill + c with decode_calls = exact_decode − 1), and `n_ctx` is the bound every
    // court cost is derived over. NOT `prefill + decode <= n_ctx`: that form is one stricter and
    // would refuse the floor's own declared worst case, (11, 2) at n_ctx 12 — footprint exactly
    // 12 under the enumeration, span 13 under the stricter reading. Two hand-written descriptions
    // of one computation is the defect class this file keeps recording.
    let footprint =
        (canonical.declared_prefill_tokens as u64).saturating_add(canonical.exact_decode_tokens.max(1) as u64).saturating_sub(1);
    if footprint > profile.n_ctx as u64 {
        return Err(PalwClassAdmissionError::Profile(format!(
            "the canonical job touches {footprint} cached positions and the class registers n_ctx {}",
            profile.n_ctx
        )));
    }
    // **ADR-0077 Decision 14, the other end of the same measurement.** The check above refuses a
    // canonical job too LONG for the row; this refuses one too SHORT for it, which is the failure
    // that costs a class 86 % of its certified work rather than refusing it outright — invisible,
    // and therefore the one that needs a gate.
    if let Some(rules) = ladder
        && footprint < rules.canonical_footprint_floor
    {
        return Err(PalwClassAdmissionError::CanonicalFootprintUnderTheRow { footprint, floor: rules.canonical_footprint_floor });
    }

    // A4 first: a class whose disputes cannot be adjudicated must not reach any later check, so
    // that a coverage gap can never be reported as some more specific failure.
    let kernel_ids = reachable_kernels_v1(profile);
    verify_catalog_coverage_v1(&PalwReachableKernelSetV1 { execution_class_id: derived_id, kernel_ids: kernel_ids.clone() })
        .map_err(|_| PalwClassAdmissionError::CoverageGap)?;
    // The catalogued set is read from the adjudication table itself, which is what
    // `verify_catalog_coverage_v1` compares against — asserted here so a future refactor that
    // pointed the gate at a hand-kept list fails a test rather than certifying quietly.
    debug_assert!(kernel_ids.is_subset(&catalogued_kernel_ids_v1()), "coverage passed against a set that is not the table");
    // **And the STRONG gate, which had no non-test caller at all** (audit H-02).
    //
    // The id check above is set inclusion: it asks whether every kernel this profile names is in
    // the table. It cannot ask whether the adjudicator has an arm for the SHAPE each node wants —
    // an op needs operands of a particular arity and width, so a node can name a catalogued id
    // while asking for something nothing can produce. `verify_profile_coverage_v1` asks the
    // adjudicator itself, node by node and for both call classes.
    //
    // Near-inert while classes could only come from genesis; ADR-0049 Decision H made post-genesis
    // registration a live, permissionless path, and then a stranger could register a class whose
    // every dispute ends `Unadjudicable` — rejected but UNSLASHED, which is unfalsifiable work on a
    // chain where bonds are supposed to be at risk. The BASE-0 profile shipped 2026-08-20 did
    // exactly this at two nodes per layer and passed the id gate.
    crate::palw_catalog_coverage::verify_profile_coverage_v1(profile).map_err(|_| PalwClassAdmissionError::CoverageGap)?;

    // **And the half a static coverage gate cannot see: is there a COURT for this leaf?**
    // (ADR-0082 Decision 1, under ADR-0049 Decision C.)
    //
    // Both gates above ask whether the adjudicator's arithmetic can re-execute every kernel the
    // graph reaches. `AttnFused` passes them — it is catalogued, and its shape arm accepts the
    // three refs and the narrowings tensor — and is still unprosecutable on a ruleset with no
    // dissection, because its terminal adjudication is a PROTOCOL and not a recompute. This is
    // where the two facts are separated, immediately after the coverage walk, so a court that
    // cannot try the leaf can never be reported as a graph that cannot be executed.
    let fused = palw_profile_has_fused_attention_v1(profile);
    if fused && court.is_none() {
        return Err(PalwClassAdmissionError::FusedAttentionNeedsTheKaryCourt);
    }

    // **And the half neither of those can see: can anybody actually PLAY this class's dispute?**
    // (ADR-0069 Decision 5.)
    //
    // Both gates above are about the GRAPH — the adjudicator's arithmetic can re-execute every
    // kernel it reaches, at every node's shape, in both call classes. A class can pass all of that
    // and still be unprosecutable, because a dispute needs a party that can assemble the evidence:
    // a refutation at the narrowed leaf, a prefix state at each rung, a close carrying the weight
    // rows. `supports_court()` is where a backend answers that today, and it is node-local, it is
    // a `bool` a family writes about itself, and it appears in no consensus rule — the launch audit
    // found it `false` on both model families while 97.8% of cadence flowed to them regardless.
    //
    // So weight — and only weight — asks for the end-to-end certificate: some family this build
    // drilled to a real conviction must cover every kernel this class reaches. Registration itself
    // is untouched, which is the whole point of putting the gate here rather than on existence:
    // an uncertified class registers at a zero share, produces, gossips and counts for liveness,
    // and takes weight later by activation when a build can prosecute it. Permissionless in, and
    // still permissionless — certification is a mechanical property of a build, not a signature.
    //
    // `certified` is an argument and is nonetheless not the caller's opinion: it must hash to the
    // `court_e2e_root` this bundle commits to, so a node that padded it would be refused. That is
    // the shape `verify_catalog_coverage_v1` could NOT use — its catalog side was an argument with
    // nothing to check it against, and a caller could pass whatever the reachable set needed — and
    // it is available here only because the commitment exists. It also keeps this function pure,
    // which matters: a gate that read process-global state would be one every test had to arrange
    // and no reader could see the inputs of.
    if *share_permille > 0 {
        let covered = crate::palw_e2e_adjudicability::family_certified_for_weight_v2(
            bundle.court_e2e_root,
            certified,
            chain_certified,
            &kernel_ids,
        )
        .map_err(|e| PalwClassAdmissionError::Profile(e.to_string()))?;
        if covered.is_none() {
            return Err(PalwClassAdmissionError::NotEndToEndCertified { share: *share_permille });
        }
    }

    let worst = match ladder {
        Some(rules) => crate::palw_step::worst_case_step_leaf_count_capped_v1(profile, rules.ladder),
        None => worst_case_step_leaf_count_v1(profile),
    }
    .map_err(|e| PalwClassAdmissionError::Profile(format!("{e:?}")))?;
    let bundle_ladder = bundle.court.max_step_leaf_count();
    if worst > bundle_ladder {
        return Err(PalwClassAdmissionError::DeeperThanTheLadder { worst, ladder: bundle_ladder });
    }

    // **Decision C: what prosecuting this class costs, against what the ruleset allows.**
    //
    // Ordered after the ladder because the two answer different halves of one question — the ladder
    // bounds how many rounds a dispute takes, these bound what a round costs — and a class that
    // fails both should be told about the deeper problem first.
    let shape = ladder.map_or_else(|| PalwCourtCostShapeV1::genesis_anchored_v1(profile), |r| r.cost_shape);
    // **One court, priced once.** The shape arrives from the caller (it is the ladder rule's, built
    // from the same `PalwKaryCourtV1` this gate holds), so the arity it prices a fused site at must
    // be the arity the ruleset froze. Checked rather than overwritten: a gate that silently
    // corrected the shape would admit a class at a price no caller could reproduce, which is the
    // same defect as pricing it twice.
    if fused && let Some(k) = court {
        let arity = bundle.court.dissection_arity();
        if shape.dissection != Some(arity) {
            return Err(PalwClassAdmissionError::PricedForADifferentCourt { priced: shape.dissection, court: arity });
        }
        let _ = k;
    }
    let cost = derive_court_cost_shaped_v1(profile, shape)?;
    // **In chunks, not bytes** (ADR-0080 design A). A close rides an `ObjectChunk` group, so what
    // a ruleset pays for is a count of carriers and half a chunk is a whole transaction. With the
    // shipped pair the two readings are the same refusal — `max_close_bytes` IS
    // `palw_close_bytes_for_chunks_v1(max_close_chunks)`, and `chunks(b) <= C` iff
    // `b <= bytes_for_chunks(C)` — and the chunk form is the one that stays true if a court is
    // ever built off the chunk grid. `PalwConsensusParamsV2::validate` compares the same unit, so
    // a class cannot be admitted at registration and refused at boot.
    for (what, got, ceiling) in [
        (
            "court close chunks",
            crate::palw_mode_v2::palw_close_chunks_for_bytes_v1(cost.max_close_bytes),
            bundle.court.max_close_chunks(),
        ),
        ("terminal multiply-accumulates", cost.max_terminal_macs, bundle.court.max_terminal_macs()),
        ("operand count", cost.max_operand_count as u64, bundle.court.max_operand_count() as u64),
    ] {
        if got > ceiling {
            return Err(PalwClassAdmissionError::CourtCostExceedsCeiling { what, got, ceiling });
        }
    }

    // **The third bound: the whole dispute inside `window_court`** (ADR-0082 Decision 3, Z4/Z11).
    //
    // The close says a move can be CARRIED and the ladder says the search TERMINATES; neither says
    // the two searches fit the clock. A fused row adds a second search — the history dissection,
    // `PALW_ATTN_HISTORY_TILE_V4` positions a tile — and at the shipped binary arity the leaf
    // ladder alone already spends 2,970 of the RC's 3,000 DAA, so this is the bound the arity
    // derivation exists to satisfy and the one a row is most likely to fail silently without.
    //
    // The window is the CALLER's because it is a lattice quantity (`PalwLatticeWindowsV1`), not a
    // `PalwConsensusParamsV2` one: the same court parameters run under the RC's 3,000-DAA window
    // and the devnet's minutes, and a bound that read one of them from a constant here would
    // refuse a devnet row for an RC reason.
    if let Some(k) = court {
        let history = if fused { profile.n_ctx as u64 } else { 0 };
        // **The tile is the CLASS's, read off the map it registered.** A v2-mapped class's chunk is
        // the whole history and its dissection has one tile; assuming
        // `PALW_ATTN_HISTORY_TILE_V4` here would price a search the class's own evidence cannot be
        // cut into. `None` — no attention cache to chunk — is a row with no history dissection,
        // and the bound is then the ladder's alone.
        let tile = crate::palw_state_chunk_map::palw_map_history_tile_positions_v1(profile, profile.n_ctx).unwrap_or(1);
        // The rule lives beside the protocol whose cost it is (stream E), and it counts the
        // ruleset's own assembly reserve — a window that is exactly full leaves no DAA to file the
        // close in.
        crate::palw_attn_court_v1::palw_attn_court_admits_row_v1(&bundle.court, history, tile, k.window_court_daa).map_err(|e| {
            match e {
                crate::palw_attn_court_v1::PalwAttnCourtError::OverrunsWindow { moves, deadline, reserve, window_court } => {
                    PalwClassAdmissionError::CourtWindowTooShort {
                        needed: moves.saturating_mul(deadline).saturating_add(reserve),
                        window: window_court,
                    }
                }
                _ => PalwClassAdmissionError::CourtWindowTooShort { needed: u64::MAX, window: k.window_court_daa },
            }
        })?;
    }

    let counted = match ladder {
        Some(rules) => crate::palw_step::step_leaf_count_capped_v1(profile, canonical, rules.ladder),
        None => step_leaf_count(profile, canonical),
    }
    .map_err(|e| PalwClassAdmissionError::Profile(format!("{e:?}")))?;
    if counted > worst {
        return Err(PalwClassAdmissionError::CanonicalDeeperThanWorstCase { canonical: counted, worst });
    }
    match pwu_rule {
        PalwPwuRuleV2::MaxPerAttempt(_) => return Err(PalwClassAdmissionError::ClassIsNotDerived),
        PalwPwuRuleV2::DerivedV1 { pwu_per_inference } => {
            if *pwu_per_inference != counted {
                return Err(PalwClassAdmissionError::PwuPerInferenceMismatch { declared: *pwu_per_inference, counted });
            }
        }
    }

    Ok(PalwClassCatalogEntryV2 {
        class_id: derived_id,
        artifact_root: *artifact_root,
        max_step_leaf_count: worst,
        canonical_step_leaf_count: counted,
        reachable_kernels: kernel_ids,
        // The cost this gate just checked, carried so the entry a registration folds is the same
        // shape a genesis catalog holds — one derivation, two doors.
        court_cost: cost,
    })
}

/// **A registration for a chain that is ALREADY RUNNING** (ADR-0049 Decision H).
///
/// A network is born with the classes its ruleset id commits to, and a class added later has no
/// such entry: nothing on chain can check its graph, its canonical job or its `pwu` rule, so the
/// object carries them and [`verify_class_admission_v2`] decides. Without a builder, a second
/// class means re-minting the network.
///
/// This replaces the withdrawn family's `family_m_post_genesis_registration_v1` (ADR-0053), and
/// the replacement is a generalization rather than a port: the old one could only ever build the
/// one black-box class its own crate pinned, because a black-box class's identity was a set of
/// runtime pins that crate held. A deterministic class's identity is its graph, which the caller
/// already has, so this builds a registration for ANY profile — the floor, a converted checkpoint,
/// whatever the node holds an artifact for.
///
/// Three things are NOT the caller's to choose, and are therefore not parameters:
///
/// * **the share** — a post-genesis entrant joins at the ruleset's minimum grantable share, and a
///   registrant naming its own permille would be donating itself a slice of every incumbent's
///   cadence. The validator refuses anything else;
/// * **the class id** — it is the profile's id. A class IS its graph;
/// * **`pwu_per_inference`** — the canonical job's counted step leaves. A declared value is checked
///   against the count, so the only thing a choice could do here is fail.
///
/// The signature is the caller's because the bond key is: this crate never sees key material.
/// Build once with an empty signature to learn the `class_id` to sign over, then again with it.
#[allow(clippy::too_many_arguments)]
pub fn palw_post_genesis_registration_v1(
    profile: PalwShapeProfileV3,
    canonical: PalwJobContextV2,
    artifact_root: Hash64,
    min_grantable_share_permille: u16,
    initial_target: u128,
    slash_value_per_pwu: u64,
    activation_daa: u64,
    registrant_bond: crate::palw_state_v2::PalwBondKeyV2,
    signature: Vec<u8>,
) -> Result<PalwConsensusObjectV2, PalwClassAdmissionError> {
    let class_id = profile.shape_profile_id();
    // Counted here, from the same canonical job the carriage carries, so the object the gate reads
    // and the number it recounts come from one value. A caller that computed the count separately
    // could hand the gate two statements about one class.
    let counted = crate::palw_step::step_leaf_count(&profile, &canonical)
        .map_err(|e| PalwClassAdmissionError::Profile(format!("the canonical job does not count against this profile: {e}")))?;
    Ok(PalwConsensusObjectV2::ClassRegistered {
        class_id,
        artifact_root,
        slash_value_per_pwu,
        pwu_rule: crate::palw_state_v2::PalwPwuRuleV2::DerivedV1 { pwu_per_inference: counted },
        initial_target,
        share_permille: min_grantable_share_permille,
        activation_daa,
        admission: Some(Box::new(crate::palw_state_v2::PalwClassAdmissionCarriageV2 {
            profile,
            canonical,
            registrant_bond,
            signature,
        })),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_base0_profile::{PALW_RC_BASE0_CANONICAL, PALW_RC_BASE0_GEOMETRY, base0_profile_v1};
    use crate::palw_mode_v2::{PalwCourtParamsV2, tests::conforming_bundle};
    use crate::palw_qwen25_profile::{PalwQwen25GeometryV1, QWEN25_1_5B, QWEN25_3B, qwen25_profile_v1};
    use crate::palw_step::PALW_STEP_MAX_LEAVES;
    use crate::palw_v2::{PALW_TRACE_COMMITMENT_VERSION_V2, trace_scheme_id_v2};

    /// The measured Qwen2.5-1.5B graph, at the `tile_len` that actually admits its own declared
    /// context.
    ///
    /// `palw_qwen25_profile` ships `tile_len` 128 with `n_ctx` 4096, and at that pair the class's
    /// LONGEST job is 132,354,910 step leaves against a `PALW_STEP_MAX_LEAVES` of 4,194,304 — so
    /// the class as declared cannot be registered at all. Coverage says nothing about this: the
    /// graph reaches ten catalogued kernels and passes A4 either way. It is the leaf count that
    /// refuses, and `the_shipped_qwen_tile_len_does_not_admit_its_own_declared_context` below is
    /// the tripwire for it.
    ///
    /// **Derived against a court with GENEROUS cost ceilings**, because what these fixtures are
    /// about is the ladder — whether a chain provisioned for the whole step space can admit a
    /// class the genesis catalog never named. Under the RC's own `max_close_bytes` no Qwen2.5
    /// geometry is admissible at any legal `tile_len`, and that is a separate fact with its own
    /// test (`no_qwen_geometry_has_a_close_a_transaction_could_carry`); binding it into this
    /// helper would make every ladder test fail for a cost reason and say nothing about ladders.
    fn qwen_admissible() -> PalwShapeProfileV3 {
        // DERIVED against the full ladder, not a hand-picked tile. It was `tile_len: 16_384` at
        // the declared 4096 context, which was admissible against a hand-written 27-node layer
        // table; the table is projected from `BASE0_LAYER_IR` now (ADR-0049 Decision F) and the
        // engine performs 38 steps, so that pair is 4,194,650 leaves against a 4,194,304 cap.
        // A literal here would be a third description of the class, rotting on its own schedule.
        let court = generous_court();
        let g = crate::palw_qwen25_profile::qwen25_admissible_geometry_v1(QWEN25_1_5B, &court)
            .expect("some pair is admissible under the over-provisioned ladder");
        qwen25_profile_v1(g).expect("the derived geometry is expressible")
    }

    /// The full ladder, with cost ceilings wide enough that only the ladder can refuse.
    /// **Deliberately over-provisioned, and deliberately not relayable.**
    ///
    /// The tests that use this fixture are about ADMISSION MECHANICS with a big class present —
    /// catalog roots, ladder sizing, share tables — not about any such class being deliverable. A
    /// network actually configured this way could assemble closes it could not relay, which is
    /// what `DEFAULT_MAX_CLOSE_BYTES` exists to refuse at registration; nothing here may be read
    /// as a statement that a class admitted under it would work.
    fn generous_court() -> PalwCourtParamsV2 {
        PalwCourtParamsV2::with_cost_ceilings(PALW_STEP_MAX_LEAVES, 20, 2, u64::MAX, u64::MAX, u32::MAX)
            .expect("a court that refuses only on depth is legal")
    }

    fn context(profile: &PalwShapeProfileV3, prefill: u32, decode: u32) -> PalwJobContextV2 {
        let mut ctx = PalwJobContextV2 {
            version: PALW_TRACE_COMMITMENT_VERSION_V2,
            network_id: b"misaka-palw-rc".to_vec(),
            job_id: Hash64::default(),
            job_nullifier: Hash64::default(),
            assignment_id: Hash64::default(),
            execution_seed: [0; 32],
            model_profile_id: Hash64::default(),
            runtime_manifest_hash: Hash64::default(),
            runtime_class_id: Hash64::default(),
            shape_profile_id: profile.shape_profile_id(),
            trace_scheme_id: Hash64::default(),
            cu_ruleset_id: Hash64::default(),
            tokenizer_id: Hash64::default(),
            prompt_token_ids_hash: Hash64::default(),
            declared_prefill_tokens: prefill,
            exact_decode_tokens: decode,
            max_context_tokens: profile.n_ctx,
        };
        ctx.trace_scheme_id = trace_scheme_id_v2();
        ctx
    }

    /// A bundle whose ladder is provisioned for the whole step space rather than for one class.
    ///
    /// This is the move the plan turns on. `max_step_leaf_count` is a bundle field and the bundle
    /// is `palw_ruleset_id_v2`, so a class deeper than the ladder cannot join a running chain —
    /// it needs a new ruleset. But the ladder is `ceil(log2(leaves)) + terminal` ROUNDS, so
    /// provisioning it at `PALW_STEP_MAX_LEAVES` covers every class that could ever be
    /// adjudicable, and costs six rounds over provisioning it for the floor alone (16 → 22).
    fn bundle_with_full_ladder() -> PalwConsensusParamsV2 {
        let mut bundle = conforming_bundle();
        bundle.court = PalwCourtParamsV2::new(PALW_STEP_MAX_LEAVES, 20, 2).expect("the full ladder is a legal court");
        bundle
    }

    /// A network that has DECIDED, at genesis, to carry a Qwen-scale class and to pay for its
    /// courts. The ceilings are the measured cost of that class rather than a round number, which
    /// is the only honest way to choose a value that is inside the ruleset id forever.
    fn bundle_that_pays_for_qwen() -> PalwConsensusParamsV2 {
        // **Measured, not typed.** The ceilings are exactly what prosecuting this class costs, so
        // the fixture cannot drift away from the class it claims to pay for — and the number it
        // prints is the finding: a network that wanted Qwen2.5-1.5B at the widest context its
        // ladder admits would have to declare a close of gigabytes, which no transaction carries.
        // That is why the RC does not choose it.
        let cost = derive_court_cost_v1(&qwen_admissible()).expect("the derived geometry has a derivable cost");
        let mut bundle = conforming_bundle();
        bundle.court = PalwCourtParamsV2::with_cost_ceilings(
            PALW_STEP_MAX_LEAVES,
            20,
            2,
            cost.max_close_bytes,
            cost.max_terminal_macs,
            cost.max_operand_count,
        )
        .expect("a court sized for a 1.5B class is legal, and expensive on purpose");
        bundle
    }

    /// **The certified family these tests admit against, and the bundle that commits to it.**
    ///
    /// Every test below is about the STATIC half of admission — ids, coverage, ladder depth, court
    /// cost, pwu derivation — so each one needs the weight gate (ADR-0069 Decision 5) to be
    /// satisfied rather than exercised. Satisfying it honestly means holding a family that covers
    /// what the class reaches AND a bundle whose `court_e2e_root` is that family's root: the gate
    /// refuses a set that does not hash to the commitment, so a test cannot wave it away by
    /// passing a bigger list.
    ///
    /// The family covers everything this build catalogs, which is the weakest claim that admits
    /// every profile these fixtures use — coverage already requires a class's kernels to be a
    /// subset of the catalog, so nothing here is admitted that A4 would not admit. The gate's
    /// REFUSAL is proven where the real registry lives (`misaka-palw-base0`'s drill, against the
    /// shipped genesis), because that is the only place a genuinely uncertified family exists.
    fn certified_for_tests() -> Vec<crate::palw_e2e_adjudicability::PalwE2eFamilyV1> {
        crate::palw_e2e_adjudicability::catalog_covering_family_for_tests_v1()
    }

    /// `verify_class_admission_v2` with the weight gate satisfied — see [`certified_for_tests`].
    /// The bundle is cloned and re-rooted rather than mutated in place so a caller's fixture keeps
    /// describing whatever it was built to describe.
    fn admit(
        bundle: &PalwConsensusParamsV2,
        profile: &PalwShapeProfileV3,
        canonical: &PalwJobContextV2,
        registration: &PalwConsensusObjectV2,
    ) -> Result<PalwClassCatalogEntryV2, PalwClassAdmissionError> {
        let certified = certified_for_tests();
        let mut bundle = bundle.clone();
        bundle.court_e2e_root = crate::palw_e2e_adjudicability::palw_court_e2e_root_of_v1(&certified);
        verify_class_admission_v2(&bundle, profile, canonical, registration, &certified)
    }

    fn registration(class_id: Hash64, pwu_per_inference: u64) -> PalwConsensusObjectV2 {
        PalwConsensusObjectV2::ClassRegistered {
            class_id,
            artifact_root: Hash64::from_u64_word(0xA271FAC7),
            slash_value_per_pwu: 1,
            pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference },
            initial_target: 1,
            share_permille: 1,
            activation_daa: 0,
            admission: None,
        }
    }

    /// **W3's number: the widest `n_ctx` each family is actually ADMITTED at, under both ladders.**
    ///
    /// The ceiling is not the gate on its own, and that is the trap this test exists to make
    /// unmissable. `verify_class_admission_v4` refuses on `max_step_leaf_count` BEFORE it prices
    /// anything, so raising `max_close_bytes` from 80 KiB to 2,250,000 widens exactly nothing
    /// while the shipped `2^22` ladder stands — `qwen25_admissible_geometry_v1`'s own `fits`
    /// closure has the same ordering, and reading the ceiling alone is how a design comes to
    /// estimate a width the gate never grants.
    ///
    /// So both gates are measured together, per family and per ladder, through the gate's own
    /// door rather than by re-deriving its arithmetic. The refusal at `widest + 1` is captured as
    /// well: a width bounded by a build failure rather than by a rule would otherwise read as a
    /// measurement.
    ///
    /// Run it with output when the number is what you want:
    /// `cargo test -p kaspa-consensus-core --lib the_widest_context_each_family_admits -- --nocapture`
    #[test]
    fn the_widest_context_each_family_admits() {
        use crate::palw_context_ladder::{
            PALW_CONTEXT_LADDER_MAX_STEP_LEAVES, palw_a16_context_row_profile_v1, palw_class_ladder_rules_v1,
            palw_qwen36_context_row_profile_v1,
        };

        // `share_permille: 0` — this measures SHAPE and COST, so ADR-0069's weight gate is out of
        // the question rather than waved past with a fixture family.
        let weightless = |class_id: Hash64, pwu_per_inference: u64| PalwConsensusObjectV2::ClassRegistered {
            class_id,
            artifact_root: Hash64::from_u64_word(0xA271FAC7),
            slash_value_per_pwu: 1,
            pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference },
            initial_target: 1,
            share_permille: 0,
            activation_daa: 0,
            admission: None,
        };

        // `close_bytes` is a parameter so the OLD ceiling can be measured by the same harness:
        // a width that moved is only evidence if the two readings differ in one number.
        let verdict = |build: fn(u32) -> Result<PalwShapeProfileV3, crate::palw_step::PalwStepError>,
                       n_ctx: u32,
                       deep: bool,
                       close_bytes: u64|
         -> Result<PalwClassCatalogEntryV2, String> {
            let profile = build(n_ctx).map_err(|e| format!("the profile does not project: {e:?}"))?;
            let rules = if deep {
                Some(palw_class_ladder_rules_v1(&profile).ok_or_else(|| "the row registers no state chunk map".to_string())?)
            } else {
                None
            };
            let mut bundle = conforming_bundle();
            // The ladder is a BUNDLE field — `verify_class_admission_v4` compares the class's worst
            // case against `bundle.court.max_step_leaf_count()`, not against `rules.ladder` — so
            // "arm the 2^32 ladder" means moving this, and `Some(rules)` alone would move nothing.
            let top = if deep { PALW_CONTEXT_LADDER_MAX_STEP_LEAVES } else { PALW_STEP_MAX_LEAVES };
            bundle.court = PalwCourtParamsV2::with_cost_ceilings(
                top,
                20,
                2,
                close_bytes,
                crate::palw_mode_v2::DEFAULT_MAX_TERMINAL_MACS,
                crate::palw_mode_v2::DEFAULT_MAX_OPERAND_COUNT,
            )
            .expect("a court at either ladder is legal");
            // A canonical job spanning the whole context: the only declaration that meets
            // Decision 14's `n_ctx / 8` floor at every width, so the footprint rule never stands in
            // front of the answer this test is about.
            let canonical = context(&profile, n_ctx.saturating_sub(2).max(1), 2);
            let counted = match &rules {
                Some(r) => crate::palw_step::step_leaf_count_capped_v1(&profile, &canonical, r.ladder),
                None => step_leaf_count(&profile, &canonical),
            }
            .map_err(|e| format!("the canonical job has no step space: {e:?}"))?;
            let registration = weightless(profile.shape_profile_id(), counted);
            verify_class_admission_v4(&bundle, &profile, &canonical, &registration, &[], &[], rules).map_err(|e| format!("{e:?}"))
        };

        let widest =
            |build: fn(u32) -> Result<PalwShapeProfileV3, crate::palw_step::PalwStepError>, deep: bool, close_bytes: u64| -> u32 {
                if verdict(build, 2, deep, close_bytes).is_err() {
                    return 0;
                }
                // Monotone in `n_ctx`: more context is strictly more leaves and never fewer opened
                // bytes, which is the same property `qwen25_admissible_geometry_v1` binary-searches on.
                let (mut lo, mut hi) = (2u32, 8_192u32);
                while lo + 1 < hi {
                    let mid = lo + (hi - lo) / 2;
                    if verdict(build, mid, deep, close_bytes).is_ok() { lo = mid } else { hi = mid }
                }
                lo
            };

        // The ceiling ADR-0080 replaces, kept as a control rather than as history.
        const OLD_CEILING: u64 = 80 * 1024;
        let now = crate::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES;

        let mut measured: Vec<(&str, bool, u64, u32, String)> = Vec::new();
        for (family, build) in [
            ("dense A16 (graph-v2)", palw_a16_context_row_profile_v1 as fn(u32) -> _),
            ("hybrid QWEN36 (graph-v3)", palw_qwen36_context_row_profile_v1 as fn(u32) -> _),
        ] {
            for deep in [false, true] {
                for close_bytes in [OLD_CEILING, now] {
                    let w = widest(build, deep, close_bytes);
                    // At `w == 0` nothing was admitted at all, and the refusal to report is the one
                    // at the narrowest context the sweep tries — n_ctx 1 is degenerate.
                    let refused_at = (w + 1).max(2);
                    let why = verdict(build, refused_at, deep, close_bytes)
                        .err()
                        .unwrap_or_else(|| "ADMITTED — the search's upper bound bound it".into());
                    println!(
                        "{family} @ ladder {} @ ceiling {close_bytes}: widest admitted n_ctx = {w}; n_ctx {refused_at} refused by {why}",
                        if deep { "2^32" } else { "2^22 (shipped)" },
                    );
                    measured.push((family, deep, close_bytes, w, why));
                }
            }
        }
        let at = |family: &str, deep: bool, close_bytes: u64| {
            measured.iter().find(|m| m.0 == family && m.1 == deep && m.2 == close_bytes).expect("swept")
        };

        // ---- Under the SHIPPED 2^22 ladder the ceiling buys almost nothing, and what it buys is
        // bounded by the ladder rather than by the price. This is the trap, as a measurement.
        assert_eq!(at("dense A16 (graph-v2)", false, OLD_CEILING).3, 21, "the width the 80 KiB carrier admitted, unfenced");
        assert_eq!(at("dense A16 (graph-v2)", false, now).3, 39, "and the width 2,250,000 admits — the LADDER stops it");
        assert!(
            at("dense A16 (graph-v2)", false, now).4.contains("TooManyLeaves"),
            "the shipped ladder stopped being the binding refusal: {}",
            at("dense A16 (graph-v2)", false, now).4
        );
        assert_eq!(at("hybrid QWEN36 (graph-v3)", false, OLD_CEILING).3, 8);
        assert_eq!(at("hybrid QWEN36 (graph-v3)", false, now).3, 12, "the hybrid does not widen at all under 2^22");

        // ---- Under a 2^32 ladder the ceiling is the gate, and this is what it grants.
        // 30 is the figure ADR-0080's motivation quotes as "the widest row the carrier admits
        // today" — reproduced here by the gate rather than recited, which is what makes the 1,002
        // below a comparable number and not a differently-derived one.
        assert_eq!(at("dense A16 (graph-v2)", true, OLD_CEILING).3, 30, "the anchored court at the old ceiling");
        assert_eq!(at("dense A16 (graph-v2)", true, now).3, 1_002, "the dense row's widest admitted context");
        // ZERO, and it is not a bug in the sweep: the anchored court charges one checkpoint
        // opening per history-reading REFERENCE and the recurrence node declares five, so an
        // 80 KiB ceiling cannot pay for the hybrid at ANY context — the floor
        // `what_still_refuses_the_hybrid_512_row` names, measured from the gate's side.
        assert_eq!(at("hybrid QWEN36 (graph-v3)", true, OLD_CEILING).3, 0, "the 80 KiB anchored court admitted a hybrid row");
        assert_eq!(at("hybrid QWEN36 (graph-v3)", true, now).3, 514, "the hybrid row's widest admitted context");
        for family in ["dense A16 (graph-v2)", "hybrid QWEN36 (graph-v3)"] {
            assert!(
                at(family, true, now).4.contains("court close chunks"),
                "{family}: past the ladder the close ceiling must be what refuses, got {}",
                at(family, true, now).4
            );
        }

        // ---- The consequence ADR-0080 was written for: BOTH of ADR-0077 Decision 13's first row
        // (`PALW_CONTEXT_LADDER_ROWS[0]` = 512) are inside the ceiling now, and neither was.
        assert_eq!(crate::palw_context_ladder::PALW_CONTEXT_LADDER_ROWS[0], 512);
        for family in ["dense A16 (graph-v2)", "hybrid QWEN36 (graph-v3)"] {
            assert!(at(family, true, now).3 >= 512, "{family}: the 512 row is still not admitted");
            assert!(at(family, true, OLD_CEILING).3 < 512, "{family}: the 512 row was admissible before — re-read ADR-0080");
        }

        // ---- **And the graph-v5 rows** (ADR-0082 Decisions 1-6, Z11). Their width is bound by the
        // LADDER or by the WINDOW and never by the close, which is Decisions 1-4's whole point —
        // and the test says which, from the gate's own refusal rather than from a claim.
        let v5 = |build: fn(u32) -> Result<PalwShapeProfileV3, crate::palw_step::PalwStepError>, n_ctx: u32| -> Result<(), String> {
            let profile = build(n_ctx).map_err(|e| format!("the profile does not project: {e:?}"))?;
            let court = kary_court_v1();
            let rules = crate::palw_context_ladder::palw_class_ladder_rules_for_court_v1(&profile, Some(court))
                .ok_or_else(|| "the row registers no state chunk map".to_string())?;
            let mut bundle = conforming_bundle();
            bundle.court = PalwCourtParamsV2::with_cost_ceilings(
                crate::palw_context_ladder::PALW_CONTEXT_LADDER_MAX_STEP_LEAVES,
                RC_TURN_DEADLINE,
                2,
                now,
                crate::palw_mode_v2::DEFAULT_MAX_TERMINAL_MACS,
                crate::palw_mode_v2::DEFAULT_MAX_OPERAND_COUNT,
            )
            .expect("a court at the deep ladder is legal")
            .with_dissection_arity(court.dissection_arity)
            .expect("the derived arity is legal");
            let canonical = context(&profile, n_ctx.saturating_sub(2).max(1), 2);
            let counted = crate::palw_step::step_leaf_count_capped_v1(&profile, &canonical, rules.ladder)
                .map_err(|e| format!("the canonical job has no step space: {e:?}"))?;
            let registration = weightless_registration(profile.shape_profile_id(), counted);
            verify_class_admission_v5(&bundle, &profile, &canonical, &registration, &[], &[], Some(rules), Some(court))
                .map(|_| ())
                .map_err(|e| format!("{e:?}"))
        };
        let widest_v5 = |build: fn(u32) -> Result<PalwShapeProfileV3, crate::palw_step::PalwStepError>| -> (u32, String) {
            let (mut lo, mut hi) = (2u32, 262_144u32);
            if v5(build, lo).is_err() {
                return (0, v5(build, lo).unwrap_err());
            }
            while lo + 1 < hi {
                let mid = lo + (hi - lo) / 2;
                if v5(build, mid).is_ok() { lo = mid } else { hi = mid }
            }
            let why = v5(build, lo + 1).err().unwrap_or_else(|| "ADMITTED — the search's bound bound it".into());
            (lo, why)
        };
        for (family, build) in [
            ("dense A16 (graph-v5)", crate::palw_context_ladder::palw_a16_context_row_profile_v5 as fn(u32) -> _),
            ("hybrid QWEN36 (graph-v5)", crate::palw_context_ladder::palw_qwen36_context_row_profile_v5 as fn(u32) -> _),
        ] {
            let (w, why) = widest_v5(build);
            println!("{family}: widest admitted n_ctx = {w}; n_ctx {} refused by {why}", w + 1);
            assert!(w >= 512, "{family}: the 512 row is not admitted at all — {why}");
            assert!(
                !why.contains("court close chunks"),
                "{family}: the CLOSE is what refuses a v5 row at {} — Decisions 1-4 exist so that it is not: {why}",
                w + 1
            );
            assert!(
                why.contains("TooManyLeaves") || why.contains("DeeperThanTheLadder") || why.contains("CourtWindowTooShort"),
                "{family}: a v5 row must be bound by the ladder or the window, got {why}"
            );
        }
    }

    /// A registration at `share_permille: 0` — every test here measures SHAPE and COST, so
    /// ADR-0069's weight gate is out of the question rather than waved past with a fixture family.
    fn weightless_registration(class_id: Hash64, pwu_per_inference: u64) -> PalwConsensusObjectV2 {
        PalwConsensusObjectV2::ClassRegistered {
            class_id,
            artifact_root: Hash64::from_u64_word(0xA271FAC7),
            slash_value_per_pwu: 1,
            pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference },
            initial_target: 1,
            share_permille: 0,
            activation_daa: 0,
            admission: None,
        }
    }

    /// The `palw_kary_court` a `PALW_RC_WINDOWS_V1`-shaped ruleset derives — every field read off
    /// the ruleset, none chosen. `palw_court_arity_v1` returns 4 at this window and clock.
    fn kary_court_v1() -> PalwKaryCourtV1 {
        PalwKaryCourtV1 {
            dissection_arity: 4,
            prompt_ids_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1,
            window_court_daa: crate::palw_fp_devnet_v3::PALW_RC_WINDOWS_V1.window_court,
        }
    }

    /// The move clock `palw_court_turn_deadline_v1` derives for the RC's 3,000-DAA court window at
    /// the `2^32` ladder — asserted against the derivation in
    /// `the_rc_derives_its_own_arity_and_the_moves_it_buys`, restated here as the constant this
    /// harness builds its court at.
    const RC_TURN_DEADLINE: u64 = 42;

    /// **Z10: a court that cannot try the leaf must not admit the class** (ADR-0082 Decision 1,
    /// under ADR-0049 Decision C), and the refusal names the COURT rather than the graph.
    #[test]
    fn a_fused_row_is_refused_where_the_kary_court_is_dormant() {
        let profile = crate::palw_context_ladder::palw_a16_context_row_profile_v5(512).expect("the v5 row projects");
        let rules = crate::palw_context_ladder::palw_class_ladder_rules_v1(&profile).expect("mapped");
        let mut bundle = conforming_bundle();
        bundle.court = PalwCourtParamsV2::with_cost_ceilings(
            crate::palw_context_ladder::PALW_CONTEXT_LADDER_MAX_STEP_LEAVES,
            RC_TURN_DEADLINE,
            2,
            crate::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES,
            crate::palw_mode_v2::DEFAULT_MAX_TERMINAL_MACS,
            crate::palw_mode_v2::DEFAULT_MAX_OPERAND_COUNT,
        )
        .expect("legal");
        let canonical = context(&profile, 510, 2);
        let counted = crate::palw_step::step_leaf_count_capped_v1(&profile, &canonical, rules.ladder).expect("counts");
        let registration = weightless_registration(profile.shape_profile_id(), counted);

        // The shipped gate — `palw_kary_court` dormant on every preset — refuses it BY NAME.
        let err = verify_class_admission_v4(&bundle, &profile, &canonical, &registration, &[], &[], Some(rules))
            .expect_err("a fused row must not be admitted by a court with no dissection");
        assert_eq!(err, PalwClassAdmissionError::FusedAttentionNeedsTheKaryCourt, "got {err}");
        assert!(format!("{err}").contains("no dissection to try it with"), "the refusal must name the court: {err}");

        // And a graph-v2 row is untouched by the same gate: this refuses the CLASS's leaf, not
        // every class on a dormant network.
        let v2 = crate::palw_context_ladder::palw_a16_context_row_profile_v1(512).expect("projects");
        let v2_rules = crate::palw_context_ladder::palw_class_ladder_rules_v1(&v2).expect("mapped");
        let v2_canonical = context(&v2, 510, 2);
        let v2_counted = crate::palw_step::step_leaf_count_capped_v1(&v2, &v2_canonical, v2_rules.ladder).expect("counts");
        let v2_reg = weightless_registration(v2.shape_profile_id(), v2_counted);
        verify_class_admission_v4(&bundle, &v2, &v2_canonical, &v2_reg, &[], &[], Some(v2_rules))
            .expect("the shipped graph-v2 row is admitted exactly as before");
    }

    /// **Z11: all three bounds at once, and the refusal names which one.**
    #[test]
    fn a_v5_row_clears_the_close_the_ladder_and_the_window_or_names_the_one_it_does_not() {
        let court = kary_court_v1();
        let profile = crate::palw_context_ladder::palw_a16_context_row_profile_v5(512).expect("projects");
        let rules = crate::palw_context_ladder::palw_class_ladder_rules_for_court_v1(&profile, Some(court)).expect("mapped");
        let canonical = context(&profile, 510, 2);
        let counted = crate::palw_step::step_leaf_count_capped_v1(&profile, &canonical, rules.ladder).expect("counts");
        let registration = weightless_registration(profile.shape_profile_id(), counted);
        let court_at = |chunks: u64, deadline: u64| {
            PalwCourtParamsV2::with_cost_ceilings(
                crate::palw_context_ladder::PALW_CONTEXT_LADDER_MAX_STEP_LEAVES,
                deadline,
                2,
                crate::palw_mode_v2::palw_close_bytes_for_chunks_v1(chunks),
                crate::palw_mode_v2::DEFAULT_MAX_TERMINAL_MACS,
                crate::palw_mode_v2::DEFAULT_MAX_OPERAND_COUNT,
            )
            .expect("legal")
            .with_dissection_arity(court.dissection_arity)
            .expect("legal")
        };
        let admit = |bundle_court: PalwCourtParamsV2, k: PalwKaryCourtV1| {
            let mut bundle = conforming_bundle();
            bundle.court = bundle_court;
            verify_class_admission_v5(&bundle, &profile, &canonical, &registration, &[], &[], Some(rules), Some(k))
        };
        // All three clear.
        admit(court_at(crate::palw_mode_v2::DEFAULT_MAX_CLOSE_CHUNKS, RC_TURN_DEADLINE), court)
            .expect("the dense graph-v5 512 row clears the close, the ladder and the window");
        // The CLOSE alone refuses, and says so: the row needs two carriers (its bottom is charged
        // at the cache-write route) and one is what the ruleset pays for.
        let err = admit(court_at(1, RC_TURN_DEADLINE), court).expect_err("one carrier does not carry this row");
        assert!(format!("{err}").contains("court close chunks"), "the close must name itself: {err}");
        // The WINDOW alone refuses, and says so.
        let err = admit(court_at(crate::palw_mode_v2::DEFAULT_MAX_CLOSE_CHUNKS, RC_TURN_DEADLINE), PalwKaryCourtV1 {
            window_court_daa: 100,
            ..court
        })
        .expect_err("a 100-DAA court window prosecutes nothing");
        assert!(matches!(err, PalwClassAdmissionError::CourtWindowTooShort { .. }), "the window must name itself: {err}");
        // And a shape priced for a court the ruleset does not play is refused rather than corrected.
        let mut mispriced = rules;
        mispriced.cost_shape.dissection = Some(64);
        let mut bundle = conforming_bundle();
        bundle.court = court_at(crate::palw_mode_v2::DEFAULT_MAX_CLOSE_CHUNKS, RC_TURN_DEADLINE);
        let err = verify_class_admission_v5(&bundle, &profile, &canonical, &registration, &[], &[], Some(mispriced), Some(court))
            .expect_err("a class priced at an arity the court does not play must not be admitted");
        assert!(matches!(err, PalwClassAdmissionError::PricedForADifferentCourt { priced: Some(64), court: 4 }), "got {err}");
    }

    /// **The two graph-v5 class ids, pinned.** They are NEW ids — the fused graph and the tiled map
    /// are both inside `shape_profile_id` — so nothing shipped moves, and pinning them is what
    /// makes that checkable rather than asserted.
    #[test]
    fn the_graph_v5_rows_have_their_own_class_ids() {
        let dense = crate::palw_context_ladder::palw_a16_context_row_profile_v5(512).expect("projects");
        let hybrid = crate::palw_context_ladder::palw_qwen36_context_row_profile_v5(512).expect("projects");
        let dense_v2 = crate::palw_context_ladder::palw_a16_context_row_profile_v1(512).expect("projects");
        let hybrid_v3 = crate::palw_context_ladder::palw_qwen36_context_row_profile_v1(512).expect("projects");
        println!("dense  graph-v5 @ 512: {}", dense.shape_profile_id());
        println!("hybrid graph-v5 @ 512: {}", hybrid.shape_profile_id());
        assert_ne!(dense.shape_profile_id(), dense_v2.shape_profile_id(), "a v5 row must be a different class");
        assert_ne!(hybrid.shape_profile_id(), hybrid_v3.shape_profile_id(), "a v5 row must be a different class");
        assert_ne!(dense.shape_profile_id(), hybrid.shape_profile_id());
        // The two facts that make them different, named rather than left to the id.
        assert!(palw_profile_has_fused_attention_v1(&dense) && palw_profile_has_fused_attention_v1(&hybrid));
        assert!(!palw_profile_has_fused_attention_v1(&dense_v2) && !palw_profile_has_fused_attention_v1(&hybrid_v3));
        assert_eq!(dense.state_chunk_map_id, crate::palw_state_chunk_map::tiled_kv_state_chunk_map_id_v3());
        assert_eq!(hybrid.state_chunk_map_id, crate::palw_state_chunk_map::hybrid_state_chunk_map_id_v3());
        // And no committed row of a v5 class is context-shaped (Z0's first half, from this side).
        for (name, profile) in [("dense", &dense), ("hybrid", &hybrid)] {
            for table in [&profile.pre_nodes, &profile.gdn_nodes, &profile.attn_nodes, &profile.post_nodes] {
                for node in table {
                    assert!(
                        !matches!(node.out_len, crate::palw_step::PalwStepOutLenV1::KvScaled { .. }),
                        "{name}: a graph-v5 row still commits a context-shaped row at {:?}",
                        node.op_kind
                    );
                }
            }
        }
    }

    /// **The scheme is class law and the gate enforces both halves of it.** A scheme this build
    /// cannot adjudicate is refused (the kernel-catalog rule, applied to the logits commitment);
    /// and the two shipped schemes are ADMITTED, so the gate separates rather than merely refuses.
    #[test]
    fn a_scheme_this_build_cannot_adjudicate_is_refused() {
        let mut profile = qwen_admissible();
        let canonical = context(&profile, 8, 4);
        let counted = step_leaf_count(&profile, &canonical).expect("counts");
        let bundle = bundle_that_pays_for_qwen();
        // Known schemes pass this gate (the flat default is the fixture's own).
        admit(&bundle, &profile, &canonical, &registration(profile.shape_profile_id(), counted))
            .expect("the flat scheme is adjudicable");
        // An invented scheme is refused BY the scheme gate, not downstream.
        profile.logits_scheme_id = Hash64::from_u64_word(0xDEAD_5C11E);
        let counted = step_leaf_count(&profile, &canonical).expect("counts");
        let err = admit(&bundle, &profile, &canonical, &registration(profile.shape_profile_id(), counted))
            .expect_err("a scheme nothing can adjudicate must not admit");
        assert!(format!("{err}").contains("cannot adjudicate"), "got {err}");
    }

    /// **The canonical job must fit the registered context — in the ENUMERATION's form.** The
    /// footprint is `prefill + exact_decode − 1` cached positions, so a job that touches exactly
    /// `n_ctx` positions is admissible and one position more is refused; the stricter
    /// `prefill + decode <= n_ctx` reading would refuse the boundary case, which is the floor's
    /// own declared worst shape.
    #[test]
    fn the_canonical_job_is_bounded_by_the_registered_context_in_the_enumerations_form() {
        // A context small enough that the probe is refused by THIS gate and not by the global
        // leaf cap — the fixture's own 4096 sits at the ladder's edge, where an oversized job
        // trips `TooManyLeaves` before the span rule can be the answer.
        let mut profile = qwen_admissible();
        profile.n_ctx = 64;
        let bundle = bundle_that_pays_for_qwen();
        // Exactly at the bound: footprint = 63 + 2 − 1 = 64 = n_ctx. Admissible.
        let at_bound = context(&profile, 63, 2);
        let counted = step_leaf_count(&profile, &at_bound).expect("the boundary job counts");
        admit(&bundle, &profile, &at_bound, &registration(profile.shape_profile_id(), counted))
            .expect("a job whose footprint is exactly n_ctx is the declared worst case, not a violation");
        // One past it: refused by the span gate, by name.
        let past = context(&profile, 64, 2);
        let counted =
            step_leaf_count(&profile, &past).expect("still enumerable — the violation is the class's bound, not the ladder's");
        let err = admit(&bundle, &profile, &past, &registration(profile.shape_profile_id(), counted))
            .expect_err("a canonical job past the registered context prices work the class never bounded");
        assert!(format!("{err}").contains("cached positions"), "got {err}");
    }

    /// **The plan's load-bearing claim, as a test.** A Qwen-scale BASE-0 class passes every gate a
    /// class must pass to join a running chain: its graph reaches only adjudicable kernels, its
    /// longest job fits a ladder provisioned at the step-space cap, and its declared pwu is the
    /// counted one.
    #[test]
    fn a_qwen_scale_class_can_join_a_chain_provisioned_for_the_step_space() {
        let profile = qwen_admissible();
        // The floor's shape of canonical job. (64, 64) needs 128 positions and the DERIVED context is
        // smaller than that — the class is admissible at a context the projected graph allows, not
        // at the one the hand-written table pretended to.
        let canonical = context(&profile, 8, 4);
        let counted = step_leaf_count(&profile, &canonical).expect("the canonical job counts");
        let entry = admit(&bundle_that_pays_for_qwen(), &profile, &canonical, &registration(profile.shape_profile_id(), counted))
            .expect("the measured Qwen2.5-1.5B class is admissible on a network that pays for its courts");

        assert_eq!(entry.class_id, profile.shape_profile_id(), "a class is its graph");
        assert_eq!(entry.canonical_step_leaf_count, counted);
        assert!(entry.max_step_leaf_count <= PALW_STEP_MAX_LEAVES, "the worst case is inside the step space");
        assert_eq!(entry.reachable_kernels.len(), 10, "the Qwen graph reaches ten of the catalog's kernels");
    }

    /// **Registering a model does not make a new network.** The property every "add an LLM" flow
    /// depends on, asserted where it can be broken.
    ///
    /// `class_catalog_root` commits to the classes a network is BORN with. A post-genesis
    /// registration is a consensus object that lands in chain state — it never re-enters the
    /// bundle — so the ruleset id, and therefore the `consensus_params_id` two nodes compare at
    /// handshake, cannot move because someone registered a class.
    ///
    /// If that ever stopped being true, adding a model would fork the network silently: every node
    /// that had not yet seen the registration would compute a different fingerprint and refuse to
    /// peer, and the failure would look like a connectivity problem rather than a rule change.
    #[test]
    fn registering_a_class_does_not_move_the_ruleset_id() {
        // The Qwen-scale class needs a network provisioned for its step space; that is a fact
        // about the CLASS, not about this property, and using the floor's bundle would fail on the
        // ladder before it ever reached the question being asked here.
        let bundle = bundle_that_pays_for_qwen();
        let before = crate::palw_mode_v2::palw_ruleset_id_v2(&bundle);

        // A second class, admitted by the chain's own gate — the same call a node makes.
        let profile = qwen_admissible();
        let canonical = context(&profile, 8, 4);
        let counted = step_leaf_count(&profile, &canonical).expect("counts");
        let object = registration(profile.shape_profile_id(), counted);
        admit(&bundle, &profile, &canonical, &object).expect("admissible");

        // The bundle is untouched by admitting it: admission RETURNS a catalog entry, it does not
        // put one into the params. The entry lives in chain state from here on.
        assert_eq!(
            crate::palw_mode_v2::palw_ruleset_id_v2(&bundle),
            before,
            "admitting a class must not move the ruleset id — a handshake would refuse across it"
        );
        assert_eq!(
            bundle.class_catalog_root,
            bundle_that_pays_for_qwen().class_catalog_root,
            "and the genesis catalog root is still the genesis one"
        );
    }
    /// The floor is not disturbed by the second class existing — the property that makes "add it
    /// later" different from "run a different network".
    #[test]
    fn admitting_a_second_class_does_not_move_the_floors_id() {
        let floor = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("the floor geometry is expressible");
        let before = floor.shape_profile_id();
        let big = qwen_admissible();
        let canonical = context(&big, 8, 4);
        let counted = step_leaf_count(&big, &canonical).expect("counts");
        admit(&bundle_that_pays_for_qwen(), &big, &canonical, &registration(big.shape_profile_id(), counted)).expect("admissible");
        assert_eq!(base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("re-derives").shape_profile_id(), before);
        assert_ne!(before, big.shape_profile_id(), "two geometries are two classes");
    }

    /// **A class whose kernel ids are all catalogued but whose SHAPE nothing can adjudicate is
    /// refused at registration** (audit H-02).
    ///
    /// The id gate is set inclusion: it asks whether every kernel a profile names is in the table.
    /// It cannot ask whether the adjudicator has an arm for the shape each node wants — an op needs
    /// operands of a particular arity, so a node can name a catalogued id while asking for
    /// something nothing can produce. `verify_profile_coverage_v1` asks the adjudicator itself, and
    /// until now had NO non-test caller.
    ///
    /// Near-inert while classes came only from genesis; ADR-0049 Decision H made post-genesis
    /// registration a live, permissionless path, and then this is a stranger registering a class
    /// whose every dispute ends `Unadjudicable` — rejected but UNSLASHED, which is unfalsifiable
    /// work on a chain where bonds are supposed to be at risk.
    #[test]
    fn a_class_the_adjudicator_cannot_serve_is_refused_even_when_every_id_is_catalogued() {
        let bundle = conforming_bundle();
        let good = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("the floor is expressible");
        let canonical = context(&good, PALW_RC_BASE0_CANONICAL.0, PALW_RC_BASE0_CANONICAL.1);

        // Strip a node's operands: same catalogued kernel id, a shape the adjudicator refuses. This
        // is the exact defect the BASE-0 profile shipped on 2026-08-20 at its attention matmuls.
        let mut lame = good.clone();
        let victim = lame.attn_nodes.iter_mut().find(|n| n.input_refs.len() >= 2).expect("a binary node exists");
        victim.input_refs.truncate(1);

        // Its kernel ids are unchanged, so the WEAK gate still passes…
        let ids = reachable_kernels_v1(&lame);
        assert!(
            verify_catalog_coverage_v1(&PalwReachableKernelSetV1 { execution_class_id: lame.shape_profile_id(), kernel_ids: ids })
                .is_ok(),
            "every id it names is still catalogued — which is why the id gate alone certified this"
        );

        // …and admission refuses it anyway, now that the strong gate is wired.
        let reg = registration(lame.shape_profile_id(), 1);
        let lame_ctx = context(&lame, PALW_RC_BASE0_CANONICAL.0, PALW_RC_BASE0_CANONICAL.1);
        assert!(
            matches!(admit(&bundle, &lame, &lame_ctx, &reg), Err(PalwClassAdmissionError::CoverageGap)),
            "a class the adjudicator cannot serve must not be registrable"
        );

        // And the honest floor still admits, so the gate is a bound rather than a blanket refusal.
        let ok = registration(good.shape_profile_id(), 7_900);
        assert!(
            !matches!(admit(&bundle, &good, &canonical, &ok), Err(PalwClassAdmissionError::CoverageGap)),
            "the floor's own graph is servable"
        );
    }

    /// A ladder provisioned for the floor alone refuses the bigger class — which is exactly why
    /// the provisioning decision has to be made at genesis and cannot be made later.
    #[test]
    fn a_floor_sized_ladder_refuses_the_bigger_class() {
        let floor = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("expressible");
        let floor_worst = worst_case_step_leaf_count_v1(&floor).expect("the floor's worst case is inside the cap");
        let mut bundle = conforming_bundle();
        bundle.court = PalwCourtParamsV2::new(floor_worst, 20, 2).expect("a floor-sized court is legal");

        let big = qwen_admissible();
        let canonical = context(&big, 8, 4);
        let counted = step_leaf_count(&big, &canonical).expect("counts");
        let err =
            admit(&bundle, &big, &canonical, &registration(big.shape_profile_id(), counted)).expect_err("the ladder cannot reach it");
        assert!(matches!(err, PalwClassAdmissionError::DeeperThanTheLadder { .. }), "got {err:?}");
    }

    /// `pwu_per_inference` is a declaration and pwu is a direct multiplier on fork-choice weight,
    /// so the count is what decides it. Overstating by one is refused.
    #[test]
    fn an_overstated_pwu_is_refused_against_the_count() {
        let profile = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("expressible");
        let canonical = context(&profile, PALW_RC_BASE0_CANONICAL.0, PALW_RC_BASE0_CANONICAL.1);
        let counted = step_leaf_count(&profile, &canonical).expect("counts");
        let err = admit(&bundle_with_full_ladder(), &profile, &canonical, &registration(profile.shape_profile_id(), counted + 1))
            .expect_err("an overstated pwu is a lie the count catches");
        assert!(matches!(err, PalwClassAdmissionError::PwuPerInferenceMismatch { .. }), "got {err:?}");
    }

    /// A class may not borrow another graph's id, and a network carrying value may not register a
    /// `MaxPerAttempt` class — the genesis loader's two rules, restated for the later lane so the
    /// two entry points cannot drift.
    #[test]
    fn the_id_must_be_the_graphs_and_the_rule_must_be_derived() {
        let profile = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("expressible");
        let canonical = context(&profile, PALW_RC_BASE0_CANONICAL.0, PALW_RC_BASE0_CANONICAL.1);
        let counted = step_leaf_count(&profile, &canonical).expect("counts");
        let bundle = bundle_with_full_ladder();

        let borrowed = admit(&bundle, &profile, &canonical, &registration(Hash64::from_u64_word(7), counted))
            .expect_err("an id that is not the graph's is refused");
        assert!(matches!(borrowed, PalwClassAdmissionError::ClassIdIsNotTheProfileId { .. }), "got {borrowed:?}");

        let mut bounded = registration(profile.shape_profile_id(), counted);
        if let PalwConsensusObjectV2::ClassRegistered { pwu_rule, .. } = &mut bounded {
            *pwu_rule = PalwPwuRuleV2::MaxPerAttempt(1_000);
        }
        let err = admit(&bundle, &profile, &canonical, &bounded).expect_err("bounded is not derived");
        assert!(matches!(err, PalwClassAdmissionError::ClassIsNotDerived), "got {err:?}");
    }

    /// **The GatedDeltaNet arm, against the court's own input rule.**
    ///
    /// This arm had never been reached by anything. Both shipped deterministic profiles set
    /// `gdn_nodes: Vec::new()` — `base0_profile_v1` and `qwen25_profile_v1` — and BASE-0's own test
    /// asserts a plain decoder-only transformer has no GatedDeltaNet arm, so the only class that
    /// would reach it is one nobody has registered yet. A cost bound that has never priced a class
    /// of a given shape is a bound about that shape only in the sense that a comment is.
    ///
    /// What it got wrong, and in the fail-open direction: `required_positions` expands a `GdnCore`
    /// step over EVERY prior position — `0..=position` within the prefill call, and all of prefill
    /// plus every decode call at a decode step — and each of those positions opens every one of the
    /// node's input refs. The derivation charged the refs ONCE, because a real GDN node wires its
    /// inputs as ordinary intra-table indices and those fell to the single-position arm. The
    /// `n_ctx` multiplier sat on `PALW_STEP_INPUT_CHECKPOINT_STATE` instead — a sentinel
    /// `canonical_input_leaves` refuses outright ("registration-opaque today"), so it priced a
    /// shape the court cannot adjudicate while under-pricing the one it can, by a factor of the
    /// context.
    ///
    /// The bound is compared against a floor that owes nothing to this function's arithmetic: the
    /// PATH bytes alone of the leaves the court actually requires, ignoring every tile's values.
    /// A real close is strictly larger than that.
    #[test]
    fn the_gdn_arm_covers_what_the_court_actually_opens() {
        use crate::palw_step_refute::canonical_input_leaves_v1;

        let p = crate::palw_step_refute::tests::profile();
        // The longest job this class admits, which is what the bound is derived over.
        let mut ctx = crate::palw_step_refute::tests::context();
        ctx.declared_prefill_tokens = p.n_ctx - 1;
        ctx.exact_decode_tokens = 2;

        // The recurrence itself, found by op rather than by slot number.
        let slot = (0..p.global_node_count())
            .find(|s| {
                p.resolve_node_slot(*s).map(|(n, _)| n.op_kind == crate::palw_step::PalwStepOpKindV1::GatedDeltaNet).unwrap_or(false)
            })
            .expect("the fixture has a GatedDeltaNet node");
        // Its last decode call, where the expansion is widest.
        let coord = crate::palw_step::PalwStepCoordinateV1 { call_index: 1, node_slot: slot, position: 0, tile_index: 0 };

        let required = canonical_input_leaves_v1(&p, &ctx, &coord).expect("the court can enumerate this step's inputs");
        let leaves: u64 = required.iter().map(|row| row.len() as u64).sum();
        assert!(
            leaves > u64::from(p.n_ctx),
            "the fixture must actually exercise the expansion — {leaves} leaves at n_ctx {}",
            p.n_ctx
        );

        // The model-free floor, in the CARRIER's own form: since the range openings landed, a
        // canonical row rides one sibling set per contiguous run, so the floor is the runs'
        // sibling bytes — counted by the same `step_range_sibling_count_v1` the verifier's walk
        // consumes, over the run structure the required set itself implies. (The per-leaf floor
        // this test first asserted became an OVERSTATEMENT the day paths were shared; a real
        // close is strictly larger than the runs' paths, and no longer larger than the leaves'.)
        let worst = worst_case_step_leaf_count_v1(&p).expect("inside the cap");
        let mut floor_bytes = 0u64;
        for row in &required {
            let mut runs: Vec<(u64, u64)> = Vec::new();
            for (idx, _) in row {
                match runs.last_mut() {
                    Some((start, len)) if *start + *len == *idx => *len += 1,
                    _ => runs.push((*idx, 1)),
                }
            }
            for (first, k) in runs {
                floor_bytes += crate::palw_step_leg::step_range_sibling_count_v1(worst, first, k) * 64;
            }
        }

        let cost = derive_court_cost_v1(&p).expect("derivable");
        assert!(
            cost.max_close_bytes >= floor_bytes,
            "the derivation charges {} for this class; the court requires {leaves} leaves whose PATHS alone \
             are {floor_bytes} bytes — the bound is below the evidence it is supposed to bound",
            cost.max_close_bytes
        );

        // **And the same shape in the sibling arm.** The recurrence's recomputation walks every
        // prior position over a `k_dim x v_dim` state per head; charging the emitted row's width
        // priced this fixture's step at 32 multiply-accumulates. The floor here owes nothing to the
        // derivation's constant: one pass over the state, at every position the court opens.
        let state_pass = u64::from(p.n_ctx) * u64::from(p.gdn_heads) * u64::from(p.gdn_head_k_dim) * u64::from(p.gdn_head_v_dim);
        assert!(
            cost.max_terminal_macs >= state_pass,
            "the derivation charges {} multiply-accumulates; one pass over the recurrence's own state costs {state_pass}",
            cost.max_terminal_macs
        );
    }

    /// **The pin arm dispatches on the class's own commitment, and that is what the floor's
    /// vocabulary is priced by.**
    ///
    /// `logits_scheme_id` rides `PalwShapeProfileV3`, so it is inside the class id: a class cannot
    /// register under the cheap scheme and produce under the expensive one. Under the FLAT scheme
    /// the pin is `decode x vocab x 4` and it is what holds `PALW_RC_BASE0_GEOMETRY`'s vocabulary
    /// at 1,024 — 2,048 is already 124% of the ceiling at `n_ctx` 12. Under the TILED scheme the
    /// pin stops binding at any vocabulary at all, and the KV history becomes the sole constraint.
    ///
    /// That is the lever, and this is where a change to it becomes visible. The floor does not pull
    /// it: `misaka-palw-base0`'s producer builds `base0_logits_trace_root_v1` and nothing else, so a
    /// floor that COMMITTED the tiled scheme could not produce the commitment it committed to —
    /// the liveness class would mint and then make no blocks. Moving it is a producer change first
    /// and a geometry change second, in that order.
    #[test]
    fn the_pin_price_follows_the_class_and_the_floors_vocabulary_follows_the_pin() {
        use crate::palw_step_refute::{flat_logits_scheme_id_v1, tiled_logits_scheme_id_v1};
        let ceiling = crate::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES;

        // The floor as it ships: flat, and inside the ceiling with the margin it was sized for.
        let floor = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("expressible");
        assert_eq!(floor.logits_scheme_id, flat_logits_scheme_id_v1(), "the floor commits the scheme its producer can build");
        // Re-frozen with the range-opening carrier — see `no_qwen_geometry_...` for the trail.
        assert_eq!(derive_court_cost_v1(&floor).expect("derivable").max_close_bytes, 52_704);

        let at = |vocab: u32, scheme| {
            let mut p =
                base0_profile_v1(crate::palw_base0_profile::PalwBase0GeometryV1 { vocab_size: vocab, ..PALW_RC_BASE0_GEOMETRY })
                    .expect("expressible");
            p.logits_scheme_id = scheme;
            derive_court_cost_v1(&p).expect("derivable").max_close_bytes
        };

        // Flat: the vocabulary IS the constraint, and one doubling leaves behind the ceiling that
        // CHOSE the floor's geometry — 80 KiB, the one-transaction number ADR-0080 replaced.
        //
        // **What this asserted before, and why it moved**: it read `> ceiling` against
        // `DEFAULT_MAX_CLOSE_BYTES`, which was the same 80 KiB. It is not any more, and 2,048 flat
        // is comfortably inside 2,250,000 — so the sentence "it is why the floor is 1,024" is now
        // a fact about HISTORY and is asserted against the number that was true when the choice
        // was made. The floor keeps its vocabulary regardless: `vocab_size` is inside
        // `shape_profile_id`, so widening it is a new class, not a cheaper one.
        const CEILING_THAT_CHOSE_THE_FLOOR: u64 = 80 * 1024;
        assert!(
            at(2_048, flat_logits_scheme_id_v1()) > CEILING_THAT_CHOSE_THE_FLOOR,
            "vocab 2,048 flat fit the 80 KiB carrier — the reason the floor is 1,024 was never the pin"
        );
        assert!(at(2_048, flat_logits_scheme_id_v1()) < ceiling, "and under ADR-0080's ceiling the pin no longer refuses it");
        assert_eq!(at(4_096, flat_logits_scheme_id_v1()), 200_160);

        // Tiled: the pin stops being the binding arm at any vocabulary, so the same close price
        // comes back however wide the head gets. Two tiles and three paths do not grow with it.
        // Re-frozen 61,040 → 39,408 with the range-opening carrier (the same move the floor's
        // own number made); the INVARIANT is the vocabulary-independence, and it is asserted as
        // such — three widths, one price.
        for vocab in [2_048u32, 4_096, 16_384] {
            assert_eq!(
                at(vocab, tiled_logits_scheme_id_v1()),
                39_408,
                "under the tiled scheme vocab {vocab} must cost what the non-pin evidence alone costs"
            );
        }
        assert!(at(16_384, tiled_logits_scheme_id_v1()) * 5 <= ceiling * 4, "and still inside the 80% rule");
    }

    /// **The GatedDeltaNet arm, against a class somebody actually built.**
    ///
    /// `the_gdn_arm_covers_what_the_court_actually_opens` checks the arm against the court's own
    /// input rule on the court's own fixture. This checks it against the real thing: Qwen3.6-35B-A3B
    /// is the class whose measured recurrence found the arm was fail-open in the first place, and a
    /// bound that priced the fixture correctly and the shipped class wrongly would be no better
    /// than the one it replaced.
    ///
    /// It is deliberately NOT an admissibility assertion. Whether this class fits the ceiling is a
    /// question about constants its authors are still deriving; what must hold today is that the
    /// derivation SEES its recurrence — that the number is large because the class is large, rather
    /// than small because an arm never fired.
    #[test]
    fn the_gdn_arm_prices_the_real_hybrid_class() {
        let g = crate::palw_qwen36_profile::QWEN36_35B_A3B;
        let p = crate::palw_qwen36_profile::qwen36_profile_v1(g).expect("the shipped hybrid geometry is expressible");
        assert!(!p.gdn_nodes.is_empty(), "the fixture must actually carry a recurrence");

        let cost = derive_court_cost_v1(&p).expect("derivable");
        // One pass over ONE HEAD's state, at every position the court can open. The whole-graph
        // floor (`x gdn_heads`) this test first asserted was superseded BY DESIGN: this class's
        // recurrence registers the head-sliced kernel, whose court replays the challenged head
        // alone — dividing the recomputation by the head count is what lets a 40-layer hybrid
        // have a context at all. The floor that still detects a dead arm is the per-head pass:
        // a derivation below it is once again pricing a row width instead of a replay.
        let head_pass = u64::from(p.n_ctx) * u64::from(p.gdn_head_k_dim) * u64::from(p.gdn_head_v_dim);
        assert!(
            cost.max_terminal_macs >= head_pass,
            "the derivation charges {} multiply-accumulates for the real hybrid class; one pass over one head's state is {head_pass}",
            cost.max_terminal_macs
        );
        // And the whole-graph form still prices a class that does NOT register the head-sliced
        // kernel — the fixture test covers that side; here the sliced kernel must be what changed
        // the price, not a vanished arm.
        assert!(
            cost.max_terminal_macs < u64::from(p.gdn_heads) * head_pass,
            "the head-sliced class must not be charged the whole-graph replay"
        );
        // And the byte side sees the context rather than one position of it.
        let one_position = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("expressible");
        assert!(
            cost.max_close_bytes > derive_court_cost_v1(&one_position).expect("derivable").max_close_bytes,
            "a 40-layer hybrid at n_ctx {} must not price below the four-layer floor",
            p.n_ctx
        );
    }

    /// **The other genesis decision, as arithmetic: the close ceiling is what a chunk GROUP can
    /// carry, and the floor is inside it with room to spare.**
    ///
    /// A close is evidence, and evidence too large for one carrier rides the `ObjectChunk` group
    /// ADR-0075 Decision 14 built. So the ceiling is `chunks x PALW_OBJECT_CHUNK_MAX_BYTES`
    /// de-framed, and the two halves are checked separately because they are two different facts:
    /// the CHUNK is what a standard transaction relays, and the COUNT is what the ruleset pays for.
    ///
    /// **What this test asserted before ADR-0080 design A**, and it was correct then: that
    /// `DEFAULT_MAX_CLOSE_BYTES x 1.20 + 18,000 <= PALW_STANDARD_TX_BYTES` — one close inside one
    /// transaction — plus the maximality clause that DOUBLING it would not fit. Both were about a
    /// close that weighs in a single payload, and a close does not any more. The maximality clause
    /// moved with the derivation: it is now the chunk that cannot be a round number larger, which
    /// is the sentence `PALW_OBJECT_CHUNK_MAX_BYTES` was always documented by.
    ///
    /// It still fails on either side: if the chunk grows past what a carrier can hold, or if the
    /// floor grows into the ceiling.
    #[test]
    fn the_close_ceiling_is_what_a_chunk_group_can_carry() {
        use crate::palw_mode_v2::{
            DEFAULT_MAX_CLOSE_BYTES, DEFAULT_MAX_CLOSE_CHUNKS, PALW_CLOSE_FRAMING_DENOMINATOR, PALW_CLOSE_FRAMING_NUMERATOR,
            PALW_STANDARD_TX_BYTES,
        };
        let chunk = crate::palw_state_v2::PALW_OBJECT_CHUNK_MAX_BYTES as u64;

        // Transient mass is `size x 4` and the mempool refuses a transaction over the standard
        // limit on EITHER mass, so this — not the 480,000 — is the number in bytes.
        assert_eq!(PALW_STANDARD_TX_BYTES, 120_000);

        // Half one: ONE CHUNK is one transaction. What has to fit beside it is a carrier the
        // challenger builds — one ML-DSA-87 input and a change output measures 7,457 bytes, and the
        // standard cap on a single signature script is 16,384, so 18,000 covers the worst carrier.
        const CARRIER_ALLOWANCE: u64 = 18_000;
        assert_eq!(chunk, 100_000);
        assert!(chunk + CARRIER_ALLOWANCE <= PALW_STANDARD_TX_BYTES, "a chunk plus its carrier must relay");
        // And it is the largest ROUND number that does: the next one up is not relayable.
        assert!(110_000 + CARRIER_ALLOWANCE > PALW_STANDARD_TX_BYTES, "100,000 is no longer maximal among round chunk sizes");

        // Half two: the COUNT, and the framing that turns counted bytes into carried ones. The
        // encoded object runs about 1.20x the bytes this ceiling counts, because every opening
        // carries its own coordinate and length prefixes (measured 90,888 borsh against 77,568).
        assert_eq!((PALW_CLOSE_FRAMING_NUMERATOR, PALW_CLOSE_FRAMING_DENOMINATOR), (12, 10));
        assert_eq!(DEFAULT_MAX_CLOSE_CHUNKS, 27);
        assert_eq!(
            DEFAULT_MAX_CLOSE_BYTES * PALW_CLOSE_FRAMING_NUMERATOR / PALW_CLOSE_FRAMING_DENOMINATOR,
            DEFAULT_MAX_CLOSE_CHUNKS * chunk,
            "the ceiling is exactly the group it is derived from, framed"
        );
        assert_eq!(DEFAULT_MAX_CLOSE_BYTES, 2_250_000);

        // The floor, under it. The geometry comment carries the sweep; this is the pin.
        let floor = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("expressible");
        let cost = derive_court_cost_v1(&floor).expect("derivable");
        // Re-frozen with the range-opening carrier — see `no_qwen_geometry_...` for the trail.
        assert_eq!(cost.max_close_bytes, 52_704, "the floor's most expensive close");
        assert_eq!(cost.max_terminal_macs, 32_768, "and what a node recomputes to close it");
        assert_eq!(cost.max_operand_count, 2);
        // **In the unit the gate compares**: the floor's worst close is ONE chunk of the 27, where
        // against the 80 KiB ceiling it was 64% of the whole budget. The floor did not move; what
        // it is measured against did, and stating it in chunks is what keeps that visible.
        assert_eq!(
            crate::palw_mode_v2::palw_close_chunks_for_bytes_v1(cost.max_close_bytes),
            1,
            "the liveness floor costs one carrier of the group's {DEFAULT_MAX_CLOSE_CHUNKS}"
        );
        assert!(cost.max_close_bytes < PALW_RC_COURT_MAX_CLOSE_BYTES, "the floor must stay inside the ceiling");
        assert!(cost.max_terminal_macs <= PALW_RC_COURT_MAX_TERMINAL_MACS);
        assert!(u64::from(cost.max_operand_count) <= u64::from(PALW_RC_COURT_MAX_OPERAND_COUNT));
    }

    /// **A ceiling that counts only the weight bytes is not a bound on anything a block carries.**
    ///
    /// The regression this exists for: `derive_court_cost_v1` used to return the artifact opening
    /// alone, and on the shipped floor at a 64/64 job that was 32,768 against a real close of
    /// 750,716. Both arms it missed are asserted here, because either one going quiet would restore
    /// the old, unfalsifiable answer.
    #[test]
    fn the_close_cost_counts_the_arms_that_actually_grow() {
        let floor = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("expressible");
        let base = derive_court_cost_v1(&floor).expect("derivable").max_close_bytes;

        // The KV history: doubling the context must roughly double the close, because the anchored
        // history is the checkpoint's state chunks and those are `2 x layers x n_ctx x kv_dim`.
        let wider = base0_profile_v1(crate::palw_base0_profile::PalwBase0GeometryV1 {
            n_ctx: PALW_RC_BASE0_GEOMETRY.n_ctx * 2,
            ..PALW_RC_BASE0_GEOMETRY
        })
        .expect("expressible");
        let wider_cost = derive_court_cost_v1(&wider).expect("derivable").max_close_bytes;
        assert!(wider_cost > base * 3 / 2, "twice the context must cost materially more: {base} -> {wider_cost}");

        // The generated-token pin: quadrupling the vocabulary must too, because
        // `base0_logits_trace_root_v1` is a flat hash and no single logits row can be opened.
        let fatter = base0_profile_v1(crate::palw_base0_profile::PalwBase0GeometryV1 {
            vocab_size: PALW_RC_BASE0_GEOMETRY.vocab_size * 4,
            ..PALW_RC_BASE0_GEOMETRY
        })
        .expect("expressible");
        let fatter_cost = derive_court_cost_v1(&fatter).expect("derivable").max_close_bytes;
        assert!(fatter_cost > base, "a bigger vocabulary must cost more: {base} -> {fatter_cost}");
    }

    /// **The genesis decision, as arithmetic.** Provisioning the ladder for the whole step space
    /// rather than for the floor alone costs eight rounds, and buys every admissible class — because
    /// `worst_case_step_leaf_count_v1` refuses anything deeper than the cap, so there is no class
    /// this ladder can fail to reach.
    #[test]
    fn provisioning_the_whole_step_space_costs_four_rounds() {
        let rounds = |leaves: u64| leaves.max(2).next_power_of_two().trailing_zeros();

        let floor = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("expressible");
        // The floor's LONGEST job — whole context as prefill — and not its declared 64/64 one,
        // which is 47,020. The ladder is checked against the longest job a class admits, so using
        // the declared one here understated the floor's own ladder by two rounds and the price of
        // provisioning by the same.
        let floor_worst = worst_case_step_leaf_count_v1(&floor).expect("the floor is inside the cap");
        // Re-measured five times, each because the declared graph grew to be the computation or
        // the class's own shape moved. The layer table's narrowings took 184,456 to 366,728; the
        // post table's narrowing added eight; declaring attention PER QUERY HEAD — the engine runs
        // score/amplify/softmax/narrow once per head and the table declared them once per layer —
        // took it to 465,040; ADR-0050's two residual gains took it to 481,424; and `n_ctx` 512 to
        // 12 (the cost ceiling's doing, see `PALW_RC_BASE0_GEOMETRY`) takes it to 8,352. The step
        // space is quadratic in the context, so this is the largest single move of the five.
        assert_eq!(floor_worst, 8_352, "the floor's longest job, measured");
        assert_eq!(rounds(floor_worst), 14);

        assert_eq!(PALW_RC_COURT_MAX_STEP_LEAF_COUNT, PALW_STEP_MAX_LEAVES);
        assert_eq!(rounds(PALW_RC_COURT_MAX_STEP_LEAF_COUNT), 22);
        assert_eq!(rounds(PALW_RC_COURT_MAX_STEP_LEAF_COUNT) - rounds(floor_worst), 8, "the price of the whole step space");

        // And it really is every class: the cap is what `worst_case_step_leaf_count_v1` enforces,
        // so a class the ladder cannot reach is a class that was already inadmissible.
        let big = qwen_admissible();
        assert!(worst_case_step_leaf_count_v1(&big).expect("inside the cap") <= PALW_RC_COURT_MAX_STEP_LEAF_COUNT);
    }

    /// **ADR-0049 Decision C, at the number a genesis actually has to look at: no Qwen2.5 geometry
    /// has a close a transaction could carry.**
    ///
    /// The claim used to be "it fits at a 125-token context". That was measured against the weight
    /// bytes alone, and a close is not its weight bytes: it carries the disputed step's KV history
    /// (anchored, the checkpoint's state chunks; un-anchored, one opening per position per ref) and,
    /// at a decode gather, every logits row of the job. Under
    /// [`crate::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES`] — which is what a standard transaction can
    /// carry, not a round number — the answer is now the same at every legal `tile_len`: no.
    ///
    /// That is what makes the ceiling free to freeze. A larger one would not buy this class; it
    /// would only admit a class whose disputes nobody could raise, which is the exact shape
    /// ADR-0049 exists to refuse. What buys Qwen is an openable logits commitment and a per-layer
    /// slice of the checkpoint — code, not a bigger number in a genesis.
    ///
    /// The earlier drafts of this test are worth remembering: one asserted "refused at EVERY tile
    /// length" and failed on tile 64, and its successor asserted a 125-token window that only
    /// existed because the metric was short. Both times the assertion was one measurement ahead of
    /// the claim.
    #[test]
    fn no_qwen_geometry_has_a_close_a_transaction_could_carry() {
        let floor = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("expressible");
        let floor_cost = derive_court_cost_v1(&floor).expect("derivable");
        // Re-frozen 61,040 → 52,704 when the carrier learned range openings: a canonical row now
        // rides one sibling set per contiguous run instead of one full path per leaf, and the
        // floor's binding row was four tiles wide. The ceiling did not move; the close got
        // cheaper, which is the direction a format change is allowed to move a frozen number.
        assert_eq!(floor_cost.max_close_bytes, 52_704, "the floor's most expensive close, measured");
        assert!(
            floor_cost.max_close_bytes * 5 <= crate::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES * 4,
            "the floor stays under 80% of the ceiling — the margin `PALW_RC_BASE0_GEOMETRY` was chosen for"
        );

        // Each tile is priced at the LONGEST context it can adjudicate, because that is the class a
        // network would actually register — a tile bought for its context and then not used for it
        // is a ceiling question nobody is asking.
        let widest_adjudicable_ctx = |tile: u32| -> u32 {
            let fits = |n_ctx: u32| {
                qwen25_profile_v1(PalwQwen25GeometryV1 { n_ctx, tile_len: tile, ..QWEN25_1_5B })
                    .ok()
                    .and_then(|p| worst_case_step_leaf_count_v1(&p).ok())
                    .is_some()
            };
            let (mut lo, mut hi) = (2u32, 1u32 << 16);
            while lo + 1 < hi {
                let mid = lo + (hi - lo) / 2;
                if fits(mid) { lo = mid } else { hi = mid }
            }
            lo
        };

        let default_bundle = bundle_with_full_ladder();
        let mut fits: Vec<u32> = Vec::new();
        for tile in [16u32, 32, 64, 128, 256, 512, 1024, 2048, 4096, 8192, 16_384] {
            let n_ctx = widest_adjudicable_ctx(tile);
            let p = qwen25_profile_v1(PalwQwen25GeometryV1 { n_ctx, tile_len: tile, ..QWEN25_1_5B }).expect("expressible");
            let cost = derive_court_cost_v1(&p).expect("derivable");
            let canonical = context(&p, 8, 4);
            let Ok(counted) = step_leaf_count(&p, &canonical) else { continue };
            match admit(&default_bundle, &p, &canonical, &registration(p.shape_profile_id(), counted)) {
                Ok(_) => {
                    assert!(cost.max_close_bytes <= crate::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES);
                    fits.push(tile);
                }
                Err(PalwClassAdmissionError::CourtCostExceedsCeiling { .. }) => {
                    assert!(cost.max_close_bytes > crate::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES);
                }
                Err(e) => panic!("tile {tile}: unexpected {e:?}"),
            }
        }
        assert!(fits.is_empty(), "no tile length gives Qwen2.5-1.5B a close a transaction could carry, got {fits:?}");

        // **The clause ADR-0080 turned over.** This read `> DEFAULT_MAX_CLOSE_BYTES` and meant
        // "not even the smallest expressible context fits" — true of the 80 KiB one-transaction
        // ceiling and false of the 2,250,000-byte chunk-group one. The cheapest expressible close
        // is 1,220,432 bytes: no TRANSACTION carries it, which is why the sweep above still finds
        // no admissible tile at each tile's widest context, and a 27-chunk GROUP does.
        let cheapest =
            derive_court_cost_v1(&qwen25_profile_v1(PalwQwen25GeometryV1 { n_ctx: 2, tile_len: 64, ..QWEN25_1_5B }).unwrap())
                .unwrap()
                .max_close_bytes;
        assert_eq!(cheapest, 1_220_432, "the cheapest close at the smallest expressible context");
        assert!(
            cheapest > crate::palw_mode_v2::PALW_STANDARD_TX_BYTES,
            "one transaction never carried this close, and still does not"
        );
        assert!(
            cheapest <= crate::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES,
            "a chunk group does — that is the whole of ADR-0080 design A"
        );

        // **The declared 4,096-token context is not adjudicable at ANY legal tile length**, and
        // that is a change: against the hand-written 27-node layer table `tile_len` 16,384 reached
        // it and cost 24 MiB an opening. The table is projected from `BASE0_LAYER_IR` now and the
        // engine performs 38 steps per layer, so the whole legal tile range — up to
        // `PALW_STEP_MAX_TILE_LEN` — is short. Swept rather than asserted at one tile, because
        // "16,384 works" was exactly the shape of claim that went stale silently.
        let mut admits_full_ctx = Vec::new();
        let mut tile = crate::palw_step::PALW_STEP_MIN_TILE_LEN;
        while tile <= crate::palw_step::PALW_STEP_MAX_TILE_LEN {
            if let Ok(p) = qwen25_profile_v1(PalwQwen25GeometryV1 { tile_len: tile, ..QWEN25_1_5B })
                && worst_case_step_leaf_count_v1(&p).is_ok()
            {
                admits_full_ctx.push(tile);
            }
            tile = tile.saturating_mul(2);
        }
        assert!(
            admits_full_ctx.is_empty(),
            "a tile length reached the declared 4096 context ({admits_full_ctx:?}) — the sizing table needs updating with it"
        );
    }

    /// **The shipped Qwen geometries do not admit their own declared context**, and coverage
    /// cannot see it.
    ///
    /// `worst_case_step_leaf_count_v1` is the whole context as prefill — the longest job a class
    /// admits — and both shipped constants are far past `PALW_STEP_MAX_LEAVES` at `tile_len` 128:
    /// 132.4 M leaves for 1.5B and 219.7 M for 3B. `tile_len` is the only knob that moves it, and
    /// measured, 1.5B needs 16,384 to reach `n_ctx` 4096 while 3B needs 65,536 — which is
    /// `PALW_STEP_MAX_TILE_LEN` exactly, so the 3B class at 4096 sits on the type's own ceiling
    /// with no headroom.
    ///
    /// This test fails the moment either constant changes, which is the point: it is a tripwire on
    /// a pair of numbers that pass every other gate.
    #[test]
    fn the_shipped_qwen_tile_len_does_not_admit_its_own_declared_context() {
        for shipped in [QWEN25_1_5B, QWEN25_3B] {
            let as_shipped = qwen25_profile_v1(shipped).expect("expressible");
            assert!(
                worst_case_step_leaf_count_v1(&as_shipped).is_err(),
                "a shipped Qwen geometry became admissible — update this tripwire and the sizing table with it"
            );
        }
        // The DERIVED geometry is admissible — that is what makes the class registrable at all.
        assert!(worst_case_step_leaf_count_v1(&qwen_admissible()).is_ok(), "the derived 1.5B geometry is inside the step space");
        // And the declared context is not reachable at the maximum legal tile either, for either
        // member. Against the hand-written table 1.5B reached 4096 at 16,384 and 3B at 65,536;
        // the projected graph is bigger and neither does.
        for shipped in [QWEN25_1_5B, QWEN25_3B] {
            let widest = qwen25_profile_v1(PalwQwen25GeometryV1 { tile_len: crate::palw_step::PALW_STEP_MAX_TILE_LEN, ..shipped })
                .expect("expressible at the maximum tile");
            assert!(
                worst_case_step_leaf_count_v1(&widest).is_err(),
                "the declared context became reachable at the maximum tile — update the sizing table"
            );
        }
    }

    /// **The prompt-id term IS the opening's size past ADR-0081 Decision 3's fence — the four
    /// numbers, printed, on the real derivation.**
    ///
    /// `prompt_ids_close_bytes_v1` has its own sweep in `palw_prompt_ids_v1`; this is the one that
    /// says the court's walk actually reads it. Same profile, same court, one field different, and
    /// the whole `max_close_bytes` moves by exactly the term's difference — which it can only do if
    /// the term is charged on the binding node and nowhere else is affected.
    ///
    /// `n_ctx` 512 / 4,096 / 32,768 are the contexts a long-context design is about; 30 is here
    /// because it is the one measured point where the opening is DEARER than the list it replaces.
    /// The floor's OWN `n_ctx` is 12.
    ///
    /// **And the term is ~0.1% of the close it sits in, at every one of them** — asserted below,
    /// because the four numbers read like headroom and are not. The floor's close is 52,704 bytes
    /// at `n_ctx` 12 against an 81,920-byte carrier; it passes the carrier at `n_ctx` 20 (85,536)
    /// and by `n_ctx` 512 it is 2,105,024, twenty-five times the carrier, of which the whole
    /// prompt-id term is 2,048. So arming this fence moves no class across the ceiling at any
    /// context, and it was never going to: what Decision 3 buys is the term's SHAPE
    /// (`log`-shaped instead of linear), which a long context needs and which the other terms —
    /// the history runs and their paths, ADR-0077 Decision 11's business — still do not have.
    #[test]
    fn the_prompt_id_term_is_the_openings_size_past_the_fence() {
        use crate::palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_ids_close_bytes_v1};
        let mut measured = Vec::new();
        for n_ctx in [30u32, 512, 4_096, 32_768] {
            let mut geometry = PALW_RC_BASE0_GEOMETRY;
            geometry.n_ctx = n_ctx;
            let profile = base0_profile_v1(geometry).expect("the floor's graph is expressible at any context");
            // The over-provisioned ladder, so only the id term can differ between the two readings.
            let mut shape = PalwCourtCostShapeV1::genesis_anchored_v1(&profile);
            shape.ladder = crate::palw_context_ladder::PALW_CONTEXT_LADDER_MAX_STEP_LEAVES;
            let flat = derive_court_cost_shaped_v1(&profile, shape).expect("the flat reading derives");
            let merkle = derive_court_cost_shaped_v1(&profile, shape.with_prompt_ids_form_v1(PalwPromptIdsFormV1::MerkleV1))
                .expect("the merkle reading derives");
            let flat_term = prompt_ids_close_bytes_v1(PalwPromptIdsFormV1::Flat, n_ctx as u64).unwrap();
            let merkle_term = prompt_ids_close_bytes_v1(PalwPromptIdsFormV1::MerkleV1, n_ctx as u64).unwrap();
            println!(
                "n_ctx {n_ctx:>6}: id term {flat_term:>7} -> {merkle_term:>4}; close {} -> {}",
                flat.max_close_bytes, merkle.max_close_bytes
            );
            // Signed, because below ~50 ids the opening's header outweighs the list it replaces
            // (208 against 120 at `n_ctx` 30) and the delta is negative — a fact the scheme states
            // out loud rather than a case to hide behind an unsigned subtraction.
            assert_eq!(
                i128::from(flat.max_close_bytes) - i128::from(merkle.max_close_bytes),
                i128::from(flat_term) - i128::from(merkle_term),
                "the whole close moved by exactly the id term at n_ctx {n_ctx}"
            );
            assert_eq!(flat.max_terminal_macs, merkle.max_terminal_macs, "the id term is bytes, never recomputation");
            measured.push((n_ctx, flat_term, merkle_term));
        }
        assert_eq!(
            measured,
            vec![(30u32, 120u64, 208u64), (512, 2_048, 472), (4_096, 16_384, 664), (32_768, 131_072, 856)],
            "the four numbers ADR-0081 Decision 3 is worth",
        );
        // **What the four numbers are NOT: admission headroom.** Every close above is already past
        // the carrier, the id term is a thousandth of it, and arming the fence leaves every one of
        // them past it. Stated as an assertion rather than a caveat, because "the prompt ids cost
        // 128 KiB at 32,768, where nothing fits" invites exactly the reading this refutes.
        for (n_ctx, flat_term, _) in &measured {
            let mut geometry = PALW_RC_BASE0_GEOMETRY;
            geometry.n_ctx = *n_ctx;
            let profile = base0_profile_v1(geometry).expect("expressible");
            let mut shape = PalwCourtCostShapeV1::genesis_anchored_v1(&profile);
            shape.ladder = crate::palw_context_ladder::PALW_CONTEXT_LADDER_MAX_STEP_LEAVES;
            // **The CARRIER, not the chunk-group ceiling.** This paragraph was written when a
            // close weighed in one transaction and `DEFAULT_MAX_CLOSE_BYTES` WAS the carrier;
            // ADR-0080 design A made the constant a group of 27 of them and left the sentence
            // pointing at the wrong number, which is the same stale reading the U-00 module's
            // three sweeps carried (`CARRIER_80K` there). One carrier, derived rather than typed.
            let carrier = crate::palw_mode_v2::palw_close_bytes_for_chunks_v1(1);
            let mut chunks = Vec::new();
            for form in [PalwPromptIdsFormV1::Flat, PalwPromptIdsFormV1::MerkleV1] {
                let close =
                    derive_court_cost_shaped_v1(&profile, shape.with_prompt_ids_form_v1(form)).expect("derives").max_close_bytes;
                assert!(
                    close > carrier,
                    "n_ctx {n_ctx} under {form:?} closes at {close}, which ONE carrier would admit — \
                     the fence would then be an admission change and needs its own gate test"
                );
                chunks.push(crate::palw_mode_v2::palw_close_chunks_for_bytes_v1(close));
            }
            // And the claim restated in the unit the gate actually compares (ADR-0080 design A):
            // the two forms land on the SAME side of `max_close_chunks`, so arming
            // `palw_prompt_ids_merkle` admits no row that was refused and refuses none that was
            // admitted. This is the sentence above, in the units the gate reads it in.
            let ceiling = crate::palw_mode_v2::DEFAULT_MAX_CLOSE_CHUNKS;
            assert_eq!(
                chunks[0] > ceiling,
                chunks[1] > ceiling,
                "n_ctx {n_ctx}: the id form flipped admission ({chunks:?} chunks against a ceiling of {ceiling}) — \
                 the fence is an admission change and needs its own gate test"
            );
            assert!(
                flat_term * 1_000 < derive_court_cost_shaped_v1(&profile, shape).expect("derives").max_close_bytes,
                "n_ctx {n_ctx}: the prompt-id term is under a thousandth of the close, so its form is a shape \
                 argument and never a ceiling one"
            );
        }
        // The floor's own context is the one that fits ONE CARRIER, and the widest BASE-0 row that
        // does is 18 — not any of the four above. Measured, so the sentence cannot drift from the
        // derivation. (The carrier again rather than `DEFAULT_MAX_CLOSE_BYTES`: 18 was taken when
        // the constant was 81,920 and one close was one transaction. Under design A's group of 27
        // the same sweep answers a different number, asserted below so both facts are stated.)
        let widest = |n_ctx: u32| {
            let mut g = PALW_RC_BASE0_GEOMETRY;
            g.n_ctx = n_ctx;
            let p = base0_profile_v1(g).expect("expressible");
            derive_court_cost_v1(&p).expect("derives").max_close_bytes
        };
        let carrier = crate::palw_mode_v2::palw_close_bytes_for_chunks_v1(1);
        assert!(widest(18) <= carrier, "n_ctx 18 fits one carrier: {}", widest(18));
        assert!(widest(20) > carrier, "n_ctx 20 does not: {}", widest(20));
        // And under the chunk group the floor's row is bounded by the GROUP, three chunks wide at
        // the four widths this test sweeps — the number the gate compares, so a reader who takes
        // "18" away from here also takes away that it is a per-transaction figure.
        assert!(
            widest(20) <= crate::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES,
            "n_ctx 20 stopped fitting the 27-carrier group: {}",
            widest(20)
        );
    }

    /// **No shipped price moves.** Both constructors say `Flat`, so a class derived through
    /// `derive_court_cost_v1` costs exactly what it cost before the form existed — asserted rather
    /// than assumed, because "the default is unchanged" is the claim every silent fork starts from.
    #[test]
    fn the_shipped_court_cost_reads_the_prompt_ids_flat() {
        use crate::palw_prompt_ids_v1::PalwPromptIdsFormV1;
        let profile = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("the floor derives");
        assert_eq!(PalwCourtCostShapeV1::genesis_anchored_v1(&profile).prompt_ids_form, PalwPromptIdsFormV1::Flat);
        assert_eq!(
            PalwCourtCostShapeV1::checkpoint_anchored_v1(&profile, 16, PALW_STEP_MAX_LEAVES, 0).prompt_ids_form,
            PalwPromptIdsFormV1::Flat
        );
        // And the shipped entry point: `derive_court_cost_v1` is `genesis_anchored_v1`, so the
        // floor's close is the flat reading down to the byte.
        let shipped = derive_court_cost_v1(&profile).expect("the floor's cost derives");
        let explicit = derive_court_cost_shaped_v1(
            &profile,
            PalwCourtCostShapeV1::genesis_anchored_v1(&profile).with_prompt_ids_form_v1(PalwPromptIdsFormV1::Flat),
        )
        .expect("the explicit flat reading derives");
        assert_eq!(shipped, explicit);
    }

    /// **The one gate, on the one family.** Every registration goes through the step-space gate:
    /// counted leaves, a full coverage walk, the ladder and the court's cost ceilings. There is no
    /// second arm to fall into (ADR-0053).
    #[test]
    fn every_registration_goes_through_the_step_space_gate() {
        let bundle = bundle_with_full_ladder();
        let profile = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("expressible");
        let canonical = context(&profile, 8, 4);
        let counted = step_leaf_count(&profile, &canonical).expect("the floor counts");
        let reg = PalwConsensusObjectV2::ClassRegistered {
            class_id: profile.shape_profile_id(),
            artifact_root: Hash64::from_u64_word(0xA1),
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: counted },
            initial_target: u128::MAX / 2,
            share_permille: 1000,
            activation_daa: 0,
            admission: None,
        };
        let entry = admit(&bundle, &profile, &canonical, &reg).expect("the floor is admissible");
        assert_eq!(entry.canonical_step_leaf_count, counted, "still counted in STEP LEAVES, not decode tokens");
        assert!(!entry.reachable_kernels.is_empty(), "and still coverage-checked");
    }
}

// =================================================================================================
// The close is priced in carriers, and two different layers cap them
// =================================================================================================

/// **The most carriers a court close can actually be filed on.**
///
/// A close too large for one carrier rides [`PalwConsensusObjectV2::CourtCloseDeclared`] and its
/// [`PalwConsensusObjectV2::CourtCloseChunk`]s in the session's own rooted table — NOT the generic
/// `ObjectChunk` group, whose arm still admits `FamilyCertified` alone. So the number here is
/// `PALW_COURT_CLOSE_MAX_CHUNKS`, the structural bound the transition enforces from the `u64`
/// arrival bitmap, and the ruleset's `PalwCourtParamsV2::max_close_chunks()` sits under it.
///
/// **This constant was 1, and it was right when it was written.** Before the close had its own
/// carriage, a split close was refused in the block that completed it — `ChunkedObjectKindNotAllowed`,
/// whose message still reads "only a FamilyCertified may ride in chunks; every other object fits
/// one carrier". Admission priced against 27 carriers the whole time, so a class whose worst close
/// needed more than one was ACCEPTED, held share, produced blocks, and became unprosecutable the
/// first time anyone tried. A dense 512-token row prices at 1,154,673 bytes = 14 carriers and would
/// have shipped exactly that at genesis.
///
/// **The reconciliation is the interesting part and it is why this constant is derived and not
/// typed.** When the carriage landed, the test below that pinned the gap did NOT go red — because
/// it was written against the `ObjectChunk` arm, and a close no longer travels that way. It passed
/// for a reason that had stopped being true: a guard agreeing with the fix for the wrong mechanism,
/// which is the failure this file keeps recording in other forms. Reading it as success would have
/// left the next widening of the price unmatched by the transport all over again. It is now bound to
/// the path a close actually takes.
pub const PALW_COURT_CLOSE_FILABLE_CHUNKS_V1: u64 = crate::palw_state_v2::PALW_COURT_CLOSE_MAX_CHUNKS as u64;

#[cfg(test)]
mod the_close_must_be_filable {
    use super::*;

    /// **Every class this tree ships must have a close somebody can actually file.**
    ///
    /// The sweep that would have caught the 512 row before it was registered. It compares against
    /// [`PALW_COURT_CLOSE_FILABLE_CHUNKS_V1`] — what the TRANSITION accepts — rather than the
    /// ruleset's `max_close_chunks`, because the ruleset number is what a class is PRICED against
    /// and the two are only the same question while both layers agree. The test below is what keeps
    /// them agreeing.
    #[test]
    fn no_shipped_row_needs_a_carrier_the_close_table_refuses() {
        let rows = crate::palw_court_deadline::palw_shipped_court_rows_v1().expect("the shipped rows project");
        assert!(!rows.is_empty(), "a court with no rows would make this test vacuous");
        for row in rows {
            let cost = derive_court_cost_v1(&row.profile).expect("a shipped row derives its cost");
            let carriers = crate::palw_mode_v2::palw_close_chunks_for_bytes_v1(cost.max_close_bytes);
            assert!(
                carriers <= PALW_COURT_CLOSE_FILABLE_CHUNKS_V1,
                "a shipped row (n_ctx {}) closes at {} bytes = {carriers} carriers and the close table files {}: \
                 admission would accept this class and nobody could ever prosecute it",
                row.profile.n_ctx,
                cost.max_close_bytes,
                PALW_COURT_CLOSE_FILABLE_CHUNKS_V1
            );
        }
    }

    /// **What is PRICED must be inside what can be FILED, and this is the only place that says so.**
    ///
    /// Admission refuses a class whose close exceeds `bundle.court.max_close_chunks()`; the
    /// transition refuses a declaration whose count exceeds `PALW_COURT_CLOSE_MAX_CHUNKS`. Nothing
    /// else compares them, and the direction matters: priced ABOVE filable is the hole that let a
    /// 14-carrier close be admitted onto a transport that carried none of it. Priced below is slack.
    #[test]
    fn the_priced_ceiling_is_inside_the_filable_one() {
        let priced = crate::palw_mode_v2::DEFAULT_MAX_CLOSE_CHUNKS;
        assert_eq!(priced, 27, "design A's ceiling moved — re-check the close table's bound before following it");
        assert_eq!(PALW_COURT_CLOSE_FILABLE_CHUNKS_V1, 32);
        assert!(
            priced <= PALW_COURT_CLOSE_FILABLE_CHUNKS_V1,
            "admission prices {priced} carriers and the close table files {}: every class between them is \
             admissible and unprosecutable",
            PALW_COURT_CLOSE_FILABLE_CHUNKS_V1
        );
        // The generic group is NOT the close's road, and a change that sent it back down this one
        // would restore the original hole silently. `FamilyCertified` keeps its own 8.
        assert_eq!(crate::palw_state_v2::PALW_OBJECT_CHUNK_MAX_COUNT, 8, "the certification lane's carriage moved");
        assert!(
            (crate::palw_state_v2::PALW_OBJECT_CHUNK_MAX_COUNT as u64) < priced,
            "the certification lane cannot carry a priced close, which is why the close has its own table"
        );
    }

    /// **The row the launch is waiting on, asserted rather than assumed.**
    ///
    /// A dense 512-token context is what makes a model's own answer wide enough to be an artifact
    /// DSL — at `n_ctx` 16 the cheapest artifact of any registered kind costs 35 tokens and nothing
    /// can be produced at all. This is the arithmetic that says the row is now registrable: it
    /// prices inside the ruleset's ceiling AND inside what the close table can file. It was true of
    /// neither before the close got its own carriage.
    ///
    /// The hybrid row is measured beside it deliberately. It is inside both too, at 27 of 27 priced
    /// carriers — 9,759 bytes of margin on a 2,250,000 ceiling, 0.43 % — so any geometry change,
    /// added term or tile move puts it over, and the failure surfaces at registration on a live
    /// chain. That is a fact about the margin, not a reason to register or refuse it; this test
    /// exists so the number is read rather than rediscovered.
    #[test]
    fn the_512_rows_are_filable_and_the_hybrid_has_almost_no_margin() {
        for (label, profile, want_carriers) in [
            ("A16 dense 512", crate::palw_context_ladder::palw_a16_context_row_profile_v1(512).expect("projects"), 14u64),
            ("QWEN36 hybrid 512", crate::palw_context_ladder::palw_qwen36_context_row_profile_v1(512).expect("projects"), 27u64),
        ] {
            let shape = crate::palw_context_ladder::palw_class_ladder_rules_v1(&profile).expect("a mapped row has rules").cost_shape;
            let close = derive_court_cost_shaped_v1(&profile, shape).expect("derives").max_close_bytes;
            let carriers = crate::palw_mode_v2::palw_close_chunks_for_bytes_v1(close);
            assert_eq!(carriers, want_carriers, "{label}: {close} bytes is {carriers} carriers, not {want_carriers}");
            assert!(carriers <= PALW_COURT_CLOSE_FILABLE_CHUNKS_V1, "{label}: the close table cannot file {carriers} carriers");
            assert!(
                carriers <= crate::palw_mode_v2::DEFAULT_MAX_CLOSE_CHUNKS,
                "{label}: admission prices {} carriers and this row needs {carriers}",
                crate::palw_mode_v2::DEFAULT_MAX_CLOSE_CHUNKS
            );
        }
    }
}
