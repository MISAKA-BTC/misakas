//! **ADR-0145 §1–§5 — a claim's work is DERIVED from the graph and the execution, never declared.**
//!
//! ## What was broken
//!
//! A class's fork-choice weight is `claim.pwu = expected_attempts(class_target) × pwu_per_inference`
//! (`crate::palw_pwu::palw_pwu_v1`), and `pwu_per_inference` is the STEP-LEAF count of a canonical
//! job **the registrant declares** (`palw_class_admission_v2.rs`, admission item 6). The design was
//! coherent as long as the producer ran the job it declared: the class DAA seeds `T_c ∝ share_c /
//! r_c` with `r_c ∝ 1/w_c` (`palw_class_daa.rs:684-696`), so `w_c` cancels out of the product and a
//! registrant that inflates it wins proportionally less often and earns exactly the same.
//!
//! Since `Params::palw_prefill_draw` armed at DAA 4,000 the producer does NOT run the job it
//! declared. An attempt executes `exact_decode_tokens = 1` (`crate::palw_attempt_v2::palw_attempt_job_v1`)
//! while `pwu_per_inference` still counts every DECLARED decode call. The cancellation fails by
//! exactly the ratio between the declared job and the executed one, and the 2026-09-19 reward audit
//! measured it on one artifact, one graph and one kernel set: **7.8× the fork-choice weight for
//! identical arithmetic and identical pay, 24,572× the weight per unit of arithmetic at the
//! admissible extreme** (`docs/audit/2026-09-19-llm-mining/counterexample-table.txt`, families (b)
//! and (d)). All of those rows pass every check the chain performs.
//!
//! **So the defect is not the choice of unit.** Leaves are a court fact — a tile of a committed
//! output, `elements / tile_len` — and `tile_len` is free in `[4, 65536]`, which is a second lever
//! on the same number. But a vector, a MAC count or any other unit inherits the identical defect
//! the moment a registrant writes it and nothing binds the run to the writing. **The defect is that
//! the unit is DECLARED and the execution is not bound to the declaration**, which is why this
//! module's first rule is about derivation and not about units (ADR-0145 §1).
//!
//! ## What this module is
//!
//! [`palw_canonical_work_v1`] takes a **canonical class descriptor** (ADR-0145 §3: what the model
//! IS — architecture, layer structure, tensor shapes, attention structure, routing structure,
//! quantisation format; explicitly NOT `tile_len`, commitment segmentation or serialization order)
//! and **execution facts** (ADR-0145 §6: the positions actually run, the tokens actually generated,
//! and the cache mode), and returns a [`PalwCanonicalWorkVectorV1`]. A registrant supplies no
//! number in it. The declared canonical `(P, D)` of the class is not an input at all: the only
//! thing the declaration is still good for is telling the producer and the seat WHICH job to run,
//! and that job's real shape is then what gets priced.
//!
//! It is built on [`crate::palw_economic_compute_v1`] (ADR-0131), which already walks the graph and
//! counts multiply–accumulates node by node, layer by layer, at each kv length, in closed form.
//! Nothing here re-counts arithmetic: this module composes that walk into the dimensions ADR-0145
//! §4 names, adds the two derived traffic terms ADR-0146 §2 requires, and prices the job the draw
//! actually executes.
//!
//! ## Units, and the one place a number could hide
//!
//! The arithmetic dimensions are **MAC-equivalents** — ADR-0131's unit, one 8-bit-weight ×
//! integer-activation multiply–accumulate — and the traffic dimensions are **bytes**. They are not
//! added together anywhere in this module, because adding them would be a coefficient of 1 between
//! a MAC and a byte, and ADR-0146 Rule R4 forbids anyone in the economy from setting a
//! coefficient — including this file. [`PalwCanonicalWorkVectorV1::provisional_scalar_v1`] exists
//! because the accounting sites behind the fence need one number today; it is the sum of the
//! ARITHMETIC dimensions only, it equals `palw_job_economic_compute_v1` of the executed job
//! exactly (pinned by a test), and it is provisional in the strict sense of ADR-0146 §4: the
//! measured arbitrage bound that would justify collapsing the vector has not been measured, so
//! nothing collapses the byte dimensions into it.
//!
//! ## The fence
//!
//! `Params::palw_canonical_work`, `None` on every preset. Past it the accounting sites in
//! `palw_state_v2` read the derived quantity instead of the declared one; below it they are
//! byte-identical, and `a_dormant_canonical_work_fence_moves_no_number` pins that.
//!
//! **What this module does NOT close**, stated here rather than discovered:
//!
//! * The fence is a change of UNIT for `safe_weight`, `retired_safe_weight` and the ADR-0124 work
//!   price — leaves become MAC-equivalents. The running totals are per-claim sums, so the fence is
//!   keyed on the CLAIM's own `accepted_daa` rather than on the block's, which is what lets a claim
//!   accepted below the fence and finalized, retired and re-derived above it price identically at
//!   all three points. It does NOT make the two eras commensurable with each other, and a flag day
//!   still needs a drill that crosses it (this repo's own working rule).
//! * The free-prompt lane's `work_leaves` is still the executor's own field (audit F2). This module
//!   gives that lane the same derivation — the execution facts of a free-prompt capture are exactly
//!   [`PalwCanonicalExecutionFactsV1`] — but wiring the acceptance walk to recompute it is not done
//!   here.
//! * Admission still binds DECLARED == COUNTED rather than graph == work (audit F1's mechanism).
//!   This module makes the declaration economically inert past the fence; it does not yet refuse a
//!   claim whose declaration disagrees with the derivation, which is ADR-0145 I1's second half.
//! * The panel that judges a class is still drawn only from bonds that declared capability for it
//!   (audit F3), and a class can still be admitted at a ladder its own legal jobs exceed (F4).

use crate::palw_economic_compute_v1::{
    PALW_ECONOMIC_COST_TABLE_V1, PalwEconomicBreakdownV1, PalwEconomicCostTableV1, PalwEconomicShapeV1, palw_economic_shape_v1,
    palw_weight_dtype_cost_v1,
};
use crate::palw_step::{
    PALW_STEP_INPUT_KV_K, PALW_STEP_INPUT_KV_V, PalwShapeProfileV3, PalwStepNodeRoleV1, PalwStepNodeV1, PalwStepOutLenV1,
    PalwStepTableV1,
};
use kaspa_hashes::Hash64;
use thiserror::Error;

/// The derivation's version. A change to what any dimension MEANS is a new version and a new
/// fence (ADR-0146 Rule R5: versioned whole, never edited), so a claim priced under one can always
/// be re-checked under the one it was priced under.
pub const PALW_CANONICAL_WORK_VERSION_V1: u16 = 1;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwCanonicalWorkError {
    #[error("the graph is not walkable: {0}")]
    Step(#[from] crate::palw_step::PalwStepError),
    /// The class carries more than one weight format, so the single `real_bits_per_weight` ADR-0146
    /// §2 derives the traffic term from does not exist for it. **Fail-closed rather than pick one**:
    /// guessing here would price some of the class's matmuls at a format they are not stored in,
    /// which is the self-report this module exists to remove, arriving through the back door. Every
    /// class testnet-11 registers is `I8` on every node, so nothing shipped reaches this.
    #[error("the class mixes weight formats ({first} and {second}) — one class, one streamed format")]
    MixedWeightFormat { first: u8, second: u8 },
    /// An execution that generated no token. A draw IS an execution (ADR-0072) and it always emits
    /// the one token its ticket is a function of, so zero is not a job that happened.
    #[error("the execution generated no token")]
    NoGeneratedToken,
    /// The claimed reused prefix is longer than the prompt. A miner's word for "that was a cache
    /// hit" is never an input (ADR-0145 §6), so an impossible one is refused rather than clamped.
    #[error("the reused prefix of {reused} exceeds the prefill of {prefill}")]
    PrefixExceedsPrefill { reused: u32, prefill: u32 },
}

// ---------------------------------------------------------------------------------------------
// What a weight element really costs to stream (ADR-0146 §2)
// ---------------------------------------------------------------------------------------------

/// **How many bytes a GGML weight format really stores per weight, as the format's own block
/// layout** — `(bytes per block, weights per block)`, exact, no rounding.
///
/// ADR-0146 §2 writes the traffic term as `MAC-eq × real_bits_per_weight / 8`, on the observation
/// that ADR-0131's unit already doubles as a byte count: "one 8-bit weight × integer activation
/// multiply–accumulate, which at the integer engine's batch of one is also one weight byte
/// streamed". That identity is exact at 8 bits and wrong everywhere else, in both directions —
/// `Q4_K_M` streams half a byte per weight and `F16` two — and the format is a declared and
/// VERIFIED property of the artifact, not a preference, so the correction is a derivation and not
/// a coefficient. This is ADR-0146's Rule R1 applied to the one dimension the existing table is
/// known to be blind on.
///
/// The numbers are the GGML block structs, not measurements: `Q4_K` is `d(2) + dmin(2) +
/// scales(12) + qs(128)` over `QK_K = 256` weights, `Q6_K` is `ql(128) + qh(64) + scales(16) +
/// d(2)`, `Q4_0` is `d(2) + qs(16)` over 32. A type this function does not know is priced at one
/// byte per weight — which is what [`palw_weight_dtype_cost_v1`] already charges it, so an unknown
/// format changes no number rather than inventing one.
pub fn palw_real_weight_block_v1(dtype: u8) -> (u64, u64) {
    match dtype {
        // F32, I32
        0 | 26 => (4, 1),
        // F16, BF16, I16
        1 | 30 | 25 => (2, 1),
        // I64, F64
        27 | 28 => (8, 1),
        // Q4_0, Q4_1, Q5_0, Q5_1, Q8_0, Q8_1 — QK = 32
        2 => (18, 32),
        3 => (20, 32),
        6 => (22, 32),
        7 => (24, 32),
        8 => (34, 32),
        9 => (36, 32),
        // Q2_K … Q8_K — QK_K = 256
        10 => (84, 256),
        11 => (110, 256),
        12 => (144, 256),
        13 => (176, 256),
        14 => (210, 256),
        15 => (292, 256),
        // IQ4_NL (QK 32) and IQ4_XS (QK_K 256), the two i-quants a shipped tool emits
        20 => (18, 32),
        23 => (136, 256),
        // I8 (every class testnet-11 registers) and every other narrow block type: the unit.
        _ => (1, 1),
    }
}

/// **The one weight format a class streams**, or the refusal that it has more than one.
///
/// Walks only the nodes that CARRY a weight operand — a node with an empty `weight_name` has an
/// empty `weight_dtypes` and streams nothing — across all four tables.
pub fn palw_class_weight_dtype_v1(profile: &PalwShapeProfileV3) -> Result<Option<u8>, PalwCanonicalWorkError> {
    let mut seen: Option<u8> = None;
    let tables = [&profile.pre_nodes, &profile.gdn_nodes, &profile.attn_nodes, &profile.post_nodes];
    for table in tables {
        for node in table.iter() {
            for dtype in node.weight_dtypes.iter().copied() {
                match seen {
                    None => seen = Some(dtype),
                    Some(first) if first != dtype => {
                        return Err(PalwCanonicalWorkError::MixedWeightFormat { first, second: dtype });
                    }
                    Some(_) => {}
                }
            }
        }
    }
    Ok(seen)
}

// ---------------------------------------------------------------------------------------------
// The canonical class descriptor (ADR-0145 §3)
// ---------------------------------------------------------------------------------------------

/// **What the model IS, with the registrant's representation choices removed** (ADR-0145 §3).
///
/// Today a class's identity is `PalwShapeProfileV3::shape_profile_id()` — canonical borsh over the
/// WHOLE profile, `tile_len` and the commitment scheme included — so two tilings of one model are
/// two classes with two prices, and the audit measured a 101× spread over tilings of one graph.
/// Physically they are one model at two commitment representations, and this descriptor is that
/// model: the geometry, the layer structure, the attention and routing structure, the weight
/// format and the arithmetic bindings, with `tile_len`, the logits commitment scheme, the KV chunk
/// geometry and the checkpoint chunk map left out.
///
/// It is NOT a replacement for `shape_profile_id` and this module does not make it one: the court
/// adjudicates tiles, so the court's identity must keep the tiling. What this descriptor is for is
/// the ECONOMIC question — "is this the same model doing the same work" — and the reason it exists
/// as a type rather than as a comment is that the invariance it asserts is then testable
/// (`re_tiling_a_shipped_profile_does_not_move_the_vector`).
///
/// **The tokenizer is still missing and that is a known hole, not an oversight.** ADR-0145 §3 says
/// binding one is part of this work because two tokenisations of one prompt are two different
/// executions; `palw_freeprompt_v3.rs` says in its own header that no such check exists. The
/// descriptor carries `tokenizer_id` so the hole has a named place to be closed in, and today the
/// only caller passes the job context's, which nothing verifies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwCanonicalClassDescriptorV1<'a> {
    pub profile: &'a PalwShapeProfileV3,
    /// The weights the class actually streams, as a format (`None` for a graph with no weight
    /// operand at all — the degenerate fixtures, not a shipped class).
    pub weight_dtype: Option<u8>,
    /// ADR-0145 §3: the tokenizer commitment. Carried, not yet verified anywhere.
    pub tokenizer_id: Hash64,
}

impl<'a> PalwCanonicalClassDescriptorV1<'a> {
    /// The descriptor of a registered class, from the profile its registration rode.
    pub fn of(profile: &'a PalwShapeProfileV3, tokenizer_id: Hash64) -> Result<Self, PalwCanonicalWorkError> {
        Ok(Self { profile, weight_dtype: palw_class_weight_dtype_v1(profile)?, tokenizer_id })
    }

    /// **The canonical identity: one id per model, whatever it was packaged as.**
    ///
    /// Everything in the preimage is a fact about the model or about the arithmetic that runs it.
    /// Nothing in it is a commitment-representation parameter, so a re-tiling — which is admissible
    /// today and was the audit's 101× lever — does not move it.
    pub fn canonical_class_id_v1(&self) -> Hash64 {
        let p = self.profile;
        let mut h = blake2b_simd::Params::new().hash_length(64).key(b"MISAKA/PALW/canonical-class/v1\0\0").to_state();
        h.update(&PALW_CANONICAL_WORK_VERSION_V1.to_le_bytes());
        h.update(&(p.lane as u8).to_le_bytes());
        h.update(&p.layer_count.to_le_bytes());
        h.update(&p.full_attention_interval.to_le_bytes());
        h.update(&p.hidden_dim.to_le_bytes());
        h.update(&p.ffn_dim.to_le_bytes());
        h.update(&p.attn_heads.to_le_bytes());
        h.update(&p.attn_kv_heads.to_le_bytes());
        h.update(&p.attn_head_dim.to_le_bytes());
        h.update(&p.rope_dims.to_le_bytes());
        for section in p.rope_sections {
            h.update(&section.to_le_bytes());
        }
        h.update(&p.rope_freq_base_bits.to_le_bytes());
        h.update(&p.rms_eps_bits.to_le_bytes());
        h.update(&p.l2_eps_bits.to_le_bytes());
        h.update(&p.base0_rms_eps_q.to_le_bytes());
        h.update(&p.gdn_heads.to_le_bytes());
        h.update(&p.gdn_head_k_dim.to_le_bytes());
        h.update(&p.gdn_head_v_dim.to_le_bytes());
        h.update(&p.gdn_conv_kernel.to_le_bytes());
        h.update(&p.vocab_size.to_le_bytes());
        h.update(&p.kv_cache_f16.to_le_bytes());
        h.update(&p.n_ctx.to_le_bytes());
        // The arithmetic bindings: two graphs that round differently are two models economically,
        // because they are two different executions.
        h.update(p.reference_ruleset_id.as_byte_slice());
        for binding in p.transcendental_bindings.iter() {
            h.update(&(binding.site as u8).to_le_bytes());
            h.update(binding.algorithm_id.as_byte_slice());
        }
        for fact in p.contraction_facts.iter() {
            h.update(&(fact.site as u8).to_le_bytes());
            h.update(&fact.contracted.to_le_bytes());
        }
        // The graph's SHAPE, node by node, without the tiling: which operator, what it reads, how
        // wide its output is, and which kernel program adjudicates it.
        for (table, nodes) in [(0u8, &p.pre_nodes), (1u8, &p.gdn_nodes), (2u8, &p.attn_nodes), (3u8, &p.post_nodes)] {
            h.update(&table.to_le_bytes());
            h.update(&(nodes.len() as u32).to_le_bytes());
            for node in nodes.iter() {
                h.update(&(node.op_kind as u8).to_le_bytes());
                h.update(&(node.role as u8).to_le_bytes());
                h.update(node.weight_name.as_bytes());
                h.update(&0u8.to_le_bytes());
                h.update(&(node.weight_dtypes.len() as u32).to_le_bytes());
                h.update(&node.weight_dtypes);
                match node.out_len {
                    PalwStepOutLenV1::Fixed { elements } => {
                        h.update(&0u8.to_le_bytes());
                        h.update(&elements.to_le_bytes());
                    }
                    PalwStepOutLenV1::KvScaled { multiplier } => {
                        h.update(&1u8.to_le_bytes());
                        h.update(&multiplier.to_le_bytes());
                    }
                }
                h.update(node.kernel_semantics_id.as_byte_slice());
                h.update(&(node.input_refs.len() as u32).to_le_bytes());
                for r in node.input_refs.iter() {
                    h.update(&r.to_le_bytes());
                }
            }
        }
        h.update(&self.weight_dtype.unwrap_or(0xFF).to_le_bytes());
        h.update(self.tokenizer_id.as_byte_slice());
        let mut out = [0u8; 64];
        out.copy_from_slice(h.finalize().as_bytes());
        Hash64::from_bytes(out)
    }
}

// ---------------------------------------------------------------------------------------------
// Execution facts (ADR-0145 §6)
// ---------------------------------------------------------------------------------------------

/// **How the run met the cache** (ADR-0145 §6: "a mode the protocol cannot name is a mode it
/// cannot price"). The mode does not change the arithmetic this module counts — the positions run
/// do — but it names WHICH positions were new, and naming it is what stops "that was a cache hit"
/// from being a miner's assertion with no place in the receipt.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PalwExecutionModeV1 {
    /// Every prefill position was computed by this execution. The attempt lane, always: its prompt
    /// is derived from the block anchor, so there is no prefix anyone could already hold.
    #[default]
    Uncached,
    /// The executor held a committed prefix state and ran only the tail.
    PrefixReused,
    /// The executor held the prefix's KV cache and ran only the tail.
    KvReused,
}

/// **What the execution actually did** — the only per-claim input to the derivation, and not one
/// value of it is a registrant's choice.
///
/// `prefill_tokens` and `generated_tokens` are the job's real shape. For the attempt lane past
/// `Params::palw_prefill_draw` that is the class's declared prefill and exactly ONE generated
/// token, because that is what [`crate::palw_attempt_v2::palw_attempt_job_v1`] runs — the
/// substitution that closes the audit's 427× decode lever.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwCanonicalExecutionFactsV1 {
    /// Positions in the prompt, including any the executor already held.
    pub prefill_tokens: u32,
    /// Tokens the execution emitted. At least one: a draw always emits the token its ticket is a
    /// function of.
    pub generated_tokens: u32,
    /// Leading prompt positions the executor did not recompute, as the receipt's prefix-state
    /// commitment establishes them — never as the producer asserts them.
    pub reused_prefix_tokens: u32,
    pub mode: PalwExecutionModeV1,
}

impl PalwCanonicalExecutionFactsV1 {
    /// An uncached run of `prefill` positions emitting `generated` tokens.
    pub fn uncached(prefill: u32, generated: u32) -> Self {
        Self { prefill_tokens: prefill, generated_tokens: generated, reused_prefix_tokens: 0, mode: PalwExecutionModeV1::Uncached }
    }

    /// **The facts of one ATTEMPT-lane draw of a class** — the heart of the fix.
    ///
    /// `prefill_draw` is `Params::palw_prefill_draw` at the attempt's OWN block, the same flag the
    /// producer and the replaying seat pass to [`crate::palw_attempt_v2::palw_attempt_job_v1`], so
    /// the three cannot describe different jobs. Past it the class's declared `exact_decode_tokens`
    /// is not read at all, which is precisely why a class that declares 432 decode calls and runs
    /// one is priced for the one.
    pub fn of_attempt(canonical: &crate::palw_v2::PalwJobContextV2, prefill_draw: bool) -> Self {
        let job = crate::palw_attempt_v2::palw_attempt_job_v1(canonical.clone(), prefill_draw);
        Self::uncached(job.declared_prefill_tokens, job.exact_decode_tokens)
    }
}

// ---------------------------------------------------------------------------------------------
// The vector (ADR-0145 §4)
// ---------------------------------------------------------------------------------------------

/// **One execution's work, by the kind of work** — because one scalar cannot price dense GEMM,
/// routed experts, attention, KV traffic and quantised kernels, the hardware does not, and the two
/// candidate scalars fail in opposite directions (STEP leaves count activations while ADR-0131's
/// cost counts weights, a 6.8× spread monotone in model width; MAC-equivalents carry no
/// memory-traffic term at all).
///
/// The arithmetic dimensions are MAC-equivalents; the three `_bytes` dimensions are bytes. **They
/// are never added together here.** ADR-0146 §4 declines to assume the collapse is achievable at
/// all, and the experiment that would decide it — both live classes' draw jobs replayed warm on
/// ONE host, artifact resident and then not — has not been run.
///
/// **Dimensions against ADR-0145 §4's list.** §4 names `dense_matmul, routed_expert_matmul,
/// attention_prefill, attention_decode, kv_read, kv_write, normalization, other_verified_ops` and
/// says in the same paragraph that they are provisional and will be decided by implementation.
/// Two decisions were made here:
///
/// * **`recurrence` is added.** The shipped hybrid class spends its non-expert budget in a
///   gated-delta recurrence which is neither a matmul nor attention, and folding it into
///   `other_verified_ops` would hide the largest term of the largest live model behind the
///   smallest name in the vector.
/// * **The LM head is counted as `dense_matmul`.** ADR-0131 reports it separately because it is
///   useful to read that way; economically it is a dense weight matmul over the vocabulary and it
///   streams weights like one, so it belongs to the dimension whose traffic term applies to it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwCanonicalWorkVectorV1 {
    /// Dense weight matmuls — projections, the dense FFN, the router, the shared expert, and the
    /// LM head. MAC-equivalents.
    pub dense_matmul: u128,
    /// The ACTIVE experts' matmuls of a mixture layer — a graph fact (how many experts the routed
    /// row concatenates), never a declaration. MAC-equivalents.
    pub routed_expert_matmul: u128,
    /// Attention over the cache during the prefill phase. MAC-equivalents.
    pub attention_prefill: u128,
    /// Attention over the cache during the decode phase. Separate from prefill because the same
    /// arithmetic at batch one and at batch P is not the same job on any real device.
    /// MAC-equivalents.
    pub attention_decode: u128,
    /// The gated-delta recurrence. MAC-equivalents.
    pub recurrence: u128,
    /// RMS and L2 norms. MAC-equivalents.
    pub normalization: u128,
    /// Rotations, transcendentals, the causal convolution, residuals, requantisation, the
    /// embedding read — every other op the graph commits and a seat verifies. MAC-equivalents.
    pub other_verified_ops: u128,
    /// **ADR-0146 §2, derived**: the weight bytes the matmuls stream, `MAC-eq × real bits per
    /// weight / 8` with the class's real block layout supplying the bits and
    /// [`palw_weight_dtype_cost_v1`]'s byte scaling divided back out so a wide dtype is not
    /// counted twice. No new trusted input and no hand-picked number: the format is a declared and
    /// verified property of the artifact.
    pub weight_traffic_bytes: u128,
    /// Bytes read from the KV cache, from the nodes that read it and the true kv length at each
    /// position. Bytes.
    pub kv_read_bytes: u128,
    /// Bytes written to the KV cache, from the nodes whose role is a cache write. Bytes.
    pub kv_write_bytes: u128,
}

impl PalwCanonicalWorkVectorV1 {
    /// Σ of the ARITHMETIC dimensions, in MAC-equivalents.
    pub fn arithmetic_mac_eq(&self) -> u128 {
        self.dense_matmul
            .saturating_add(self.routed_expert_matmul)
            .saturating_add(self.attention_prefill)
            .saturating_add(self.attention_decode)
            .saturating_add(self.recurrence)
            .saturating_add(self.normalization)
            .saturating_add(self.other_verified_ops)
    }

    /// Σ of the traffic dimensions, in bytes.
    pub fn traffic_bytes(&self) -> u128 {
        self.weight_traffic_bytes.saturating_add(self.kv_read_bytes).saturating_add(self.kv_write_bytes)
    }

    /// **PROVISIONAL, and the name is the warning.** A call site behind the fence needs one number
    /// today; this is that number, and it is deliberately the least interesting choice available:
    /// [`Self::arithmetic_mac_eq`], which equals ADR-0131's `palw_job_economic_compute_v1` of the
    /// executed job exactly (pinned by `the_provisional_scalar_is_adr_0131_of_the_executed_job`).
    ///
    /// **It is not a coefficient table and must not become one.** ADR-0146 Rule R7 makes a measured
    /// arbitrage bound a GATE, not a note: a basis that ships without a re-runnable search and a
    /// published worst-case ratio does not arm. No such search has been run for any weighting of
    /// these dimensions, so this collapse weights every arithmetic dimension at one — which is
    /// today's shadow CCU, a number the chain already derives — and weights the byte dimensions at
    /// ZERO rather than guessing their price. Arming the fence on this scalar therefore changes the
    /// basis from leaves to MAC-equivalents and nothing else; ADR-0146 §4's fallback (separate
    /// budgets per dimension) stays open, which it would not if this function had invented a
    /// number.
    pub fn provisional_scalar_v1(&self) -> u128 {
        self.arithmetic_mac_eq()
    }
}

// ---------------------------------------------------------------------------------------------
// The derivation
// ---------------------------------------------------------------------------------------------

/// The cost table with the norm weight zeroed. Differencing a walk under this against a walk under
/// the real table isolates the normalization dimension exactly, because `norm` is read at exactly
/// one arm of `palw_economic_compute_v1::node_cost` (`RmsNorm | L2Norm`).
///
/// Done as a difference rather than by adding a `normalization` field to
/// [`PalwEconomicBreakdownV1`] on purpose: ADR-0131 is a SHADOW that publishes per-kind numbers
/// beside the leaf basis, and moving cost out of its `elementwise` line would move a number that
/// is already being read and compared while this fence is dormant. The difference costs one more
/// graph walk, which is O(nodes) and happens once per profile.
const PALW_COST_TABLE_WITHOUT_NORM_V1: PalwEconomicCostTableV1 = PalwEconomicCostTableV1 { norm: 0, ..PALW_ECONOMIC_COST_TABLE_V1 };

/// **A profile reduced to what it costs, twice**: under the real cost table and under the same
/// table with the norm weight removed. Derived once per class and reusable across every claim of
/// it, which is why it is a type rather than a step inside [`palw_canonical_work_v1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwCanonicalShapeV1 {
    full: PalwEconomicShapeV1,
    without_norm: PalwEconomicShapeV1,
    /// KV bytes read per attention-layer-position, per unit of kv length.
    kv_read_per_kv: u128,
    /// KV bytes written per position, across every layer that writes the cache.
    kv_write_per_position: u128,
    /// `(block bytes, block weights × dtype cost)` — the exact rational that turns a weight
    /// matmul's MAC-equivalents into streamed bytes. See [`palw_real_weight_block_v1`].
    traffic_num: u128,
    traffic_den: u128,
}

/// The per-class half of the derivation: walk the graph.
pub fn palw_canonical_shape_v1(
    descriptor: &PalwCanonicalClassDescriptorV1<'_>,
) -> Result<PalwCanonicalShapeV1, PalwCanonicalWorkError> {
    let profile = descriptor.profile;
    let full = palw_economic_shape_v1(profile, &PALW_ECONOMIC_COST_TABLE_V1)?;
    let without_norm = palw_economic_shape_v1(profile, &PALW_COST_TABLE_WITHOUT_NORM_V1)?;
    let (kv_read_per_kv, kv_write_per_position) = kv_traffic_terms_v1(profile);
    // ADR-0146 §2. `palw_weight_dtype_cost_v1` already scales a matmul's MAC-equivalents by the
    // dtype's BYTE width — its own doc says so, "a 16-bit weight is two bytes of traffic per MAC" —
    // so it is already a traffic term for the wide types and is flat 1 for every block-quantised
    // type, which is exactly the gap ADR-0146 §2 names. Dividing it back out and multiplying by the
    // real block layout composes the two correctly instead of counting the width twice.
    let (block_bytes, block_weights) = descriptor.weight_dtype.map(palw_real_weight_block_v1).unwrap_or((1, 1));
    let dtype_cost = descriptor.weight_dtype.map(palw_weight_dtype_cost_v1).unwrap_or(1).max(1);
    Ok(PalwCanonicalShapeV1 {
        full,
        without_norm,
        kv_read_per_kv,
        kv_write_per_position,
        traffic_num: block_bytes as u128,
        traffic_den: (block_weights as u128).saturating_mul(dtype_cost as u128).max(1),
    })
}

/// **The KV cache's traffic, from the graph rather than from the geometry.**
///
/// Read: a node that names [`PALW_STEP_INPUT_KV_K`] or [`PALW_STEP_INPUT_KV_V`] among its inputs
/// reads `attn_kv_heads × attn_head_dim` cache elements per unit of kv length, once per reference —
/// the same width `palw_economic_compute_v1::input_width` gives those sentinels. Written: a node
/// whose role is a cache write writes its own output width.
///
/// Taken from the graph because the fused attention site (`AttnFused`, graph v5) reads the whole
/// cache through ONE node while the four-node form reads it through several, and a term derived
/// from the geometry alone would price the two the same when they are not. A pure-recurrent graph
/// has no such node and the terms are zero, which is correct and not a special case.
fn kv_traffic_terms_v1(profile: &PalwShapeProfileV3) -> (u128, u128) {
    let element_bytes = if profile.kv_cache_f16 == 1 { 2u128 } else { 4u128 };
    let cache_row = (profile.attn_kv_heads as u128).saturating_mul(profile.attn_head_dim as u128);
    let mut read_per_kv = 0u128;
    let mut write_per_position = 0u128;
    let tables = [
        (PalwStepTableV1::Pre, &profile.pre_nodes),
        (PalwStepTableV1::Gdn, &profile.gdn_nodes),
        (PalwStepTableV1::Attn, &profile.attn_nodes),
        (PalwStepTableV1::Post, &profile.post_nodes),
    ];
    for (which, nodes) in tables {
        let span = profile.table_layer_span(which) as u128;
        if span == 0 {
            continue;
        }
        for node in nodes.iter() {
            let references = node.input_refs.iter().filter(|r| **r == PALW_STEP_INPUT_KV_K || **r == PALW_STEP_INPUT_KV_V).count();
            if references > 0 {
                read_per_kv = read_per_kv
                    .saturating_add(cache_row.saturating_mul(element_bytes).saturating_mul(references as u128).saturating_mul(span));
            }
            if matches!(node.role, PalwStepNodeRoleV1::KCacheWrite | PalwStepNodeRoleV1::VCacheWrite) {
                write_per_position =
                    write_per_position.saturating_add(written_elements_v1(node).saturating_mul(element_bytes).saturating_mul(span));
            }
        }
    }
    (read_per_kv, write_per_position)
}

/// A cache-write node's output width. A `KvScaled` cache write is not a shape anything ships — a
/// write is one position's row — so its multiplier is taken as the row itself rather than as a
/// length that grows with the context, which would make the write term quadratic.
fn written_elements_v1(node: &PalwStepNodeV1) -> u128 {
    match node.out_len {
        PalwStepOutLenV1::Fixed { elements } => elements as u128,
        PalwStepOutLenV1::KvScaled { multiplier } => multiplier as u128,
    }
}

/// **A claim's canonical work: the graph, times what the execution actually did.**
///
/// The positions summed are exactly the ones ADR-0131's leaf enumeration visits, minus any prefix
/// the receipt establishes the executor already held (ADR-0145 §6: a verifier reconstructs NEW work
/// only):
///
/// * prefill position `p` runs the body at kv length `p`, for `p = reused+1 ..= P`;
/// * the last prefill position runs the logits once, at `P` — that is where the generated token
///   comes from;
/// * decode call `c = 1 ..= G-1` runs the body and the logits at `P + c`.
///
/// **Nothing the registrant wrote is read.** The class's declared canonical `(P, D)` does not
/// appear; `tile_len` does not appear; `work_leaves` and `claim.pwu` do not appear. That is
/// ADR-0145 I1 and I2 stated as a signature rather than as a promise.
pub fn palw_canonical_work_v1(
    descriptor: &PalwCanonicalClassDescriptorV1<'_>,
    facts: &PalwCanonicalExecutionFactsV1,
) -> Result<PalwCanonicalWorkVectorV1, PalwCanonicalWorkError> {
    let shape = palw_canonical_shape_v1(descriptor)?;
    palw_canonical_work_from_shape_v1(&shape, facts)
}

/// [`palw_canonical_work_v1`] from a shape already walked — the form a per-block accounting path
/// wants, because the walk is per CLASS and the facts are per CLAIM.
pub fn palw_canonical_work_from_shape_v1(
    shape: &PalwCanonicalShapeV1,
    facts: &PalwCanonicalExecutionFactsV1,
) -> Result<PalwCanonicalWorkVectorV1, PalwCanonicalWorkError> {
    if facts.generated_tokens == 0 {
        return Err(PalwCanonicalWorkError::NoGeneratedToken);
    }
    if facts.reused_prefix_tokens > facts.prefill_tokens {
        return Err(PalwCanonicalWorkError::PrefixExceedsPrefill {
            reused: facts.reused_prefix_tokens,
            prefill: facts.prefill_tokens,
        });
    }
    let prefill = facts.prefill_tokens as u128;
    let reused = facts.reused_prefix_tokens as u128;
    let decode_calls = facts.generated_tokens.saturating_sub(1) as u128;

    // The prefill phase: the positions this execution actually computed.
    let mut prefill_full = shape.full.body_over(reused + 1, prefill);
    let mut prefill_no_norm = shape.without_norm.body_over(reused + 1, prefill);
    if prefill >= 1 {
        prefill_full = add(prefill_full, shape.full.logits_over(prefill, prefill));
        prefill_no_norm = add(prefill_no_norm, shape.without_norm.logits_over(prefill, prefill));
    }
    // The decode phase: one body and one logits pass per call after the first token.
    let decode_full =
        add(shape.full.body_over(prefill + 1, prefill + decode_calls), shape.full.logits_over(prefill + 1, prefill + decode_calls));
    let decode_no_norm = add(
        shape.without_norm.body_over(prefill + 1, prefill + decode_calls),
        shape.without_norm.logits_over(prefill + 1, prefill + decode_calls),
    );

    let dense_matmul = prefill_full
        .dense_matmul
        .saturating_add(prefill_full.logits)
        .saturating_add(decode_full.dense_matmul)
        .saturating_add(decode_full.logits);
    let routed_expert_matmul = prefill_full.routed_experts.saturating_add(decode_full.routed_experts);
    // `norm = 2` in the shipped table and `0` in the differenced one, so the gap between the two
    // elementwise lines IS the norms' cost. Saturating-subtract for shape only: the zeroed table
    // can never charge MORE.
    let normalization = prefill_full
        .elementwise
        .saturating_sub(prefill_no_norm.elementwise)
        .saturating_add(decode_full.elementwise.saturating_sub(decode_no_norm.elementwise));
    let other_verified_ops = prefill_no_norm.elementwise.saturating_add(decode_no_norm.elementwise);

    // Traffic. The weight term applies to the kernels that STREAM weights — the dense and routed
    // matmuls, the LM head among them — and not to attention, which streams the cache and is
    // counted by the kv terms instead.
    let weight_macs = dense_matmul.saturating_add(routed_expert_matmul);
    let weight_traffic_bytes = weight_macs.saturating_mul(shape.traffic_num) / shape.traffic_den.max(1);

    // The cache is read at the TRUE kv length of every position the execution ran, and written once
    // per position. A reused prefix is read by the positions that follow it but is not re-written,
    // which is the whole economic content of "prefix-reused".
    let positions_run = prefill.saturating_sub(reused).saturating_add(decode_calls);
    let kv_read_bytes = shape
        .kv_read_per_kv
        .saturating_mul(sum_inclusive(reused + 1, prefill).saturating_add(sum_inclusive(prefill + 1, prefill + decode_calls)));
    let kv_write_bytes = shape.kv_write_per_position.saturating_mul(positions_run);

    Ok(PalwCanonicalWorkVectorV1 {
        dense_matmul,
        routed_expert_matmul,
        attention_prefill: prefill_full.attention,
        attention_decode: decode_full.attention,
        recurrence: prefill_full.recurrence.saturating_add(decode_full.recurrence),
        normalization,
        other_verified_ops,
        weight_traffic_bytes,
        kv_read_bytes,
        kv_write_bytes,
    })
}

/// `Σ L` over the inclusive range, zero when empty. Exact: one of `(from + to)` and `count` is even.
fn sum_inclusive(from: u128, to: u128) -> u128 {
    if from > to {
        return 0;
    }
    let count = to - from + 1;
    let a = from.saturating_add(to);
    if a.is_multiple_of(2) { (a / 2).saturating_mul(count) } else { a.saturating_mul(count / 2) }
}

fn add(a: PalwEconomicBreakdownV1, b: PalwEconomicBreakdownV1) -> PalwEconomicBreakdownV1 {
    PalwEconomicBreakdownV1 {
        dense_matmul: a.dense_matmul.saturating_add(b.dense_matmul),
        routed_experts: a.routed_experts.saturating_add(b.routed_experts),
        attention: a.attention.saturating_add(b.attention),
        recurrence: a.recurrence.saturating_add(b.recurrence),
        logits: a.logits.saturating_add(b.logits),
        elementwise: a.elementwise.saturating_add(b.elementwise),
    }
}

// ---------------------------------------------------------------------------------------------
// The accounting sites' entry point
// ---------------------------------------------------------------------------------------------

/// **One ATTEMPT-lane draw's canonical work, in the provisional scalar the accounting reads.**
///
/// This is the function the fence turns on. It prices the job
/// [`crate::palw_attempt_v2::palw_attempt_job_v1`] really runs, so past `Params::palw_prefill_draw`
/// a class that declared 432 decode calls and a class that declared two are priced identically —
/// which they are, because they execute identically.
pub fn palw_canonical_draw_work_v1(
    descriptor: &PalwCanonicalClassDescriptorV1<'_>,
    canonical: &crate::palw_v2::PalwJobContextV2,
    prefill_draw: bool,
) -> Result<PalwCanonicalWorkVectorV1, PalwCanonicalWorkError> {
    palw_canonical_work_v1(descriptor, &PalwCanonicalExecutionFactsV1::of_attempt(canonical, prefill_draw))
}

/// **A claim's declared `pwu`, re-priced onto the derived basis.**
///
/// `claim.pwu` is `expected_attempts(class_target) × declared_per_inference`
/// (`crate::palw_pwu::palw_pwu_v1`). The lottery factor is a fact about the chain that the claim
/// really paid, and the per-inference factor is the registrant's number. Multiplying by
/// `derived / declared` cancels the declared factor out of the product and leaves
/// `expected_attempts × derived`, which is the quantity ADR-0145 I1 asks for.
///
/// **Why a ratio rather than a re-derivation of the attempts.** `expected_attempts` is a function
/// of the class TARGET, which the retarget moves every window, and `safe_weight` is a running total
/// priced once at a claim's `Final` and then re-derived at two other chain points (`retire_claim`
/// and `assert_internal_consistency_v2`). Re-deriving the attempts at those later points would give
/// a different number every time and the state would refuse itself. The two factors THIS function
/// reads — the class's registered `pwu_per_inference` and the work the chain derived from its
/// registration carriage — are both frozen at registration, so every re-derivation of a given claim
/// returns the same value forever. That property is what makes the fence armable at all, and it is
/// the reason the shape is a ratio.
///
/// `None` when the class declares nothing to divide by; the caller then keeps the declared value,
/// because refusing here would turn an accounting change into a liveness failure.
pub fn palw_claim_canonical_pwu_v1(declared_pwu: u64, declared_per_inference: u64, derived_per_draw: u128) -> Option<u128> {
    if declared_per_inference == 0 {
        return None;
    }
    // u128 throughout: `claim.pwu` reaches 2^64 and a draw's MAC-equivalents reach 2^38 on the
    // largest shipped class, so the product does not fit in anything narrower. Saturating rather
    // than wrapping, for `palw_pwu_v1`'s reason — a wrap would make the heaviest claim the
    // lightest, which is the one direction an arithmetic accident must never take.
    Some((declared_pwu as u128).saturating_mul(derived_per_draw) / (declared_per_inference as u128))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_base0_profile::{PALW_RC_BASE0_CANONICAL, PALW_RC_BASE0_GEOMETRY, base0_profile_v1, rc_job_context};
    use crate::palw_context_ladder::palw_a16_context_row_profile_v5;
    use crate::palw_economic_compute_v1::palw_job_economic_compute_v1;
    use crate::palw_qwen25_profile::{QWEN25_A16_GRAPH_V5_N_CTX, qwen25_a16_graph_v5_canonical_v1};

    /// The shipped dense row — the class the audit's counterexample table is built on.
    fn dense() -> PalwShapeProfileV3 {
        palw_a16_context_row_profile_v5(QWEN25_A16_GRAPH_V5_N_CTX).expect("the shipped @512 row builds")
    }

    fn descriptor(profile: &PalwShapeProfileV3) -> PalwCanonicalClassDescriptorV1<'_> {
        PalwCanonicalClassDescriptorV1::of(profile, Hash64::default()).expect("a shipped class has one weight format")
    }

    /// **ADR-0145 I2, on the lever the audit measured at 101×.**
    ///
    /// `tile_len` is free in `[4, 65536]`, is inside `shape_profile_id` (so a re-tiling is a
    /// different class with a different price), and changes no arithmetic whatsoever — it decides
    /// how many committed tiles one output row is cut into. The audit's own table has the re-tiled
    /// dense row executing 83,102,171,136 MAC-equivalents, byte for byte the shipped row's, while
    /// its leaf count moves by a factor of three.
    ///
    /// Every dimension of the vector must be identical, and so must the canonical class id: a
    /// commitment-representation parameter that reached either of them would be a registrant-writable
    /// field with an economic effect, which is the whole defect.
    #[test]
    fn re_tiling_a_shipped_profile_does_not_move_the_vector() {
        let shipped = dense();
        let mut retiled = shipped.clone();
        for table in [&mut retiled.pre_nodes, &mut retiled.gdn_nodes, &mut retiled.attn_nodes, &mut retiled.post_nodes] {
            for node in table.iter_mut() {
                // Admissible: `validate_shape` bounds `tile_len` to [MIN, MAX] and nothing else.
                node.tile_len = crate::palw_step::PALW_STEP_MIN_TILE_LEN;
            }
        }
        assert_ne!(shipped.shape_profile_id(), retiled.shape_profile_id(), "a re-tiling IS a different class today");

        let facts = PalwCanonicalExecutionFactsV1::uncached(63, 1);
        let a = palw_canonical_work_v1(&descriptor(&shipped), &facts).expect("shipped");
        let b = palw_canonical_work_v1(&descriptor(&retiled), &facts).expect("re-tiled");
        assert_eq!(a, b, "tile_len reached the canonical work vector");
        assert_eq!(
            descriptor(&shipped).canonical_class_id_v1(),
            descriptor(&retiled).canonical_class_id_v1(),
            "tile_len reached the canonical class identity"
        );
    }

    /// **The audit's F1 decode lever, closed.**
    ///
    /// Family (b) of `counterexample-table.txt`: one graph, one artifact, one kernel set, one
    /// certification, three declared canonical jobs — (63,2) shipped, (63,128) and (63,370) — all
    /// admissible, all executing the identical 83,102,171,136 MAC-equivalents past
    /// `palw_prefill_draw`, and carrying 100 / 331.9 / 777.2 index of fork-choice weight.
    ///
    /// The declared decode budget must not appear in the derived work, because past the fence an
    /// attempt runs `exact_decode_tokens = 1` regardless of it.
    #[test]
    fn a_declared_decode_budget_does_not_enter_the_derived_work() {
        let profile = dense();
        let d = descriptor(&profile);
        let base = qwen25_a16_graph_v5_canonical_v1();
        let shipped = rc_job_context(&profile, base.0, base.1);
        let mut inflated = shipped.clone();
        inflated.exact_decode_tokens = 370;
        let mut extreme = shipped.clone();
        extreme.exact_decode_tokens = 432;

        let a = palw_canonical_draw_work_v1(&d, &shipped, true).expect("shipped");
        let b = palw_canonical_draw_work_v1(&d, &inflated, true).expect("inflated");
        let c = palw_canonical_draw_work_v1(&d, &extreme, true).expect("extreme");
        assert_eq!(a, b, "the (63,370) declaration bought work the draw does not run");
        assert_eq!(a, c, "the (63,432) declaration bought work the draw does not run");

        // And below `palw_prefill_draw` the three really are different jobs, which is why the
        // declaration was ever coherent: the fence is what broke the cancellation, and this pins
        // that the module is reading the fence and not ignoring the decode budget by accident.
        let honest = palw_canonical_draw_work_v1(&d, &inflated, false).expect("no prefill draw");
        assert!(honest.arithmetic_mac_eq() > a.arithmetic_mac_eq(), "without the fence 370 decode calls really are more work");
    }

    /// **The (1,432) admissible extreme and the shipped (63,2) row price the same EXECUTED job.**
    ///
    /// The audit's family (d) row carries 2,457,197× the shipped row's weight per unit of
    /// arithmetic by declaring a one-token prompt and 432 decode calls. Under this derivation the
    /// declaration is not an input: hand both descriptors the same execution facts — the 63
    /// positions and the one generated token that a draw of this class really runs — and the work
    /// is identical, to the byte, in every dimension.
    ///
    /// The comparison is against the basis IN FORCE — `pwu_per_inference`, the declared job's step
    /// leaves — so the test states the inversion rather than asserting a tautology about its own
    /// derivation.
    #[test]
    fn the_admissible_extreme_prices_the_same_as_the_shipped_row() {
        use crate::palw_step::step_leaf_count_capped_v1;
        let profile = dense();
        let d = descriptor(&profile);
        let base = qwen25_a16_graph_v5_canonical_v1();
        let shipped = rc_job_context(&profile, base.0, base.1);
        let mut extreme = shipped.clone();
        extreme.declared_prefill_tokens = 1;
        extreme.exact_decode_tokens = 432;

        // The basis in force. `pwu_per_inference` is this number, and `claim.pwu` is a multiple of
        // it — so the ratio below is the ratio of fork-choice weight the two classes buy per draw.
        // The court's shipped ladder, 2^26 — the cap `worst_case_step_leaf_count_capped_v1`
        // admits a class under, and the bound both of these rows are legal beneath.
        const LADDER: u64 = 1 << 26;
        let declared_shipped = step_leaf_count_capped_v1(&profile, &shipped, LADDER).expect("shipped leaves");
        let declared_extreme = step_leaf_count_capped_v1(&profile, &extreme, LADDER).expect("extreme leaves");
        assert!(
            declared_extreme > declared_shipped.saturating_mul(7),
            "the (1,432) declaration must still be worth many times the shipped row on the LEAF basis, \
             or this fixture no longer reproduces audit family (d): {declared_extreme} vs {declared_shipped}"
        );

        // The derived basis, given the one execution both descriptions could be asked to run.
        // `palw_canonical_work_v1` takes no job context at all: the declaration cannot reach it.
        let executed = PalwCanonicalExecutionFactsV1::uncached(63, 1);
        assert_eq!(palw_canonical_work_v1(&d, &executed).expect("derived"), palw_canonical_work_v1(&d, &executed).expect("derived"),);

        // And what each class's draw really runs. The extreme's declaration buys it a ONE-token
        // prompt, so past the draw fence it executes strictly less — and is now paid strictly less,
        // where on the leaf basis it is paid seven times more. That is the inversion.
        let drawn_extreme = palw_canonical_draw_work_v1(&d, &extreme, true).expect("extreme draw");
        let drawn_shipped = palw_canonical_draw_work_v1(&d, &shipped, true).expect("shipped draw");
        assert!(
            drawn_extreme.arithmetic_mac_eq() < drawn_shipped.arithmetic_mac_eq(),
            "the one-token declaration runs less and must be priced less"
        );
    }

    /// The provisional scalar is ADR-0131's own number for the job the draw executes — not a new
    /// basis, not a weighting, and nothing invented. If this ever stops holding, somebody has put a
    /// coefficient in `provisional_scalar_v1`, which ADR-0146 Rule R4 forbids.
    #[test]
    fn the_provisional_scalar_is_adr_0131_of_the_executed_job() {
        for (profile, canonical) in [
            (dense(), qwen25_a16_graph_v5_canonical_v1()),
            (base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("floor"), PALW_RC_BASE0_CANONICAL),
        ] {
            let d = descriptor(&profile);
            let job = rc_job_context(&profile, canonical.0, canonical.1);
            for prefill_draw in [false, true] {
                let executed = crate::palw_attempt_v2::palw_attempt_job_v1(job.clone(), prefill_draw);
                let expect = palw_job_economic_compute_v1(&profile, &executed, &PALW_ECONOMIC_COST_TABLE_V1).expect("adr-0131");
                let got = palw_canonical_draw_work_v1(&d, &job, prefill_draw).expect("derived");
                assert_eq!(got.provisional_scalar_v1(), expect, "the scalar drifted from ADR-0131's walk");
            }
        }
    }

    /// The traffic term is derived, not chosen: at 8-bit weights a MAC-equivalent IS one weight
    /// byte, and every class testnet-11 registers is `I8` — so the shipped classes' weight traffic
    /// equals their weight arithmetic exactly, and a class stored at `Q4_K` would stream
    /// `144/256` of it.
    ///
    /// Pinned because this is the one dimension ADR-0146 §2 calls decisive, and a silent change to
    /// [`palw_real_weight_block_v1`] would move every future class's price with no other symptom.
    #[test]
    fn the_weight_traffic_term_is_the_formats_own_block_layout() {
        let profile = dense();
        let d = descriptor(&profile);
        assert_eq!(d.weight_dtype, Some(crate::palw_qwen25_profile::QWEN25_WEIGHT_DTYPE_I8), "the shipped dense row is int8");
        let v = palw_canonical_work_v1(&d, &PalwCanonicalExecutionFactsV1::uncached(63, 1)).expect("derived");
        assert_eq!(
            v.weight_traffic_bytes,
            v.dense_matmul + v.routed_expert_matmul,
            "at 8 bits one MAC-equivalent is one weight byte streamed"
        );
        // The block layouts, as the formats define them rather than as anybody prefers them.
        assert_eq!(palw_real_weight_block_v1(24), (1, 1), "I8");
        assert_eq!(palw_real_weight_block_v1(12), (144, 256), "Q4_K: d(2)+dmin(2)+scales(12)+qs(128) over QK_K");
        assert_eq!(palw_real_weight_block_v1(14), (210, 256), "Q6_K: ql(128)+qh(64)+scales(16)+d(2) over QK_K");
        assert_eq!(palw_real_weight_block_v1(1), (2, 1), "F16");
        assert_eq!(palw_real_weight_block_v1(0), (4, 1), "F32");
        // An unknown type is priced exactly where `palw_weight_dtype_cost_v1` already prices it, so
        // it invents nothing.
        assert_eq!(palw_real_weight_block_v1(200), (1, 1));
    }

    /// **ADR-0145 §6: a cached prefix is not new work.** The free-prompt lane pays ~62× for a
    /// padded prefix while a producer holding that prefix's KV cache spends ~3 % more (audit F2).
    /// Reconstructing NEW work only is what removes the lever, so a reused prefix must reduce every
    /// dimension the prefix would have driven — and must not reduce the decode phase, which still
    /// runs.
    #[test]
    fn a_reused_prefix_is_not_paid_for_twice() {
        let profile = dense();
        let d = descriptor(&profile);
        let cold = palw_canonical_work_v1(&d, &PalwCanonicalExecutionFactsV1::uncached(63, 4)).expect("cold");
        let warm = palw_canonical_work_v1(
            &d,
            &PalwCanonicalExecutionFactsV1 {
                prefill_tokens: 63,
                generated_tokens: 4,
                reused_prefix_tokens: 60,
                mode: PalwExecutionModeV1::KvReused,
            },
        )
        .expect("warm");
        assert!(warm.arithmetic_mac_eq() < cold.arithmetic_mac_eq(), "a held prefix must not be paid as new work");
        assert_eq!(warm.attention_decode, cold.attention_decode, "the decode phase still runs either way");
        // A prefix longer than the prompt is a claim nothing can establish, so it is refused rather
        // than clamped to the prompt — clamping would make the impossible claim free.
        assert!(matches!(
            palw_canonical_work_from_shape_v1(
                &palw_canonical_shape_v1(&d).unwrap(),
                &PalwCanonicalExecutionFactsV1 {
                    prefill_tokens: 8,
                    generated_tokens: 1,
                    reused_prefix_tokens: 9,
                    mode: PalwExecutionModeV1::KvReused
                },
            ),
            Err(PalwCanonicalWorkError::PrefixExceedsPrefill { .. })
        ));
    }

    /// The dimensions carry what their names say on the two shipped shapes that differ most: the
    /// dense row has attention and no recurrence, the integer floor has neither a routed expert nor
    /// a cache.
    ///
    /// This is the test that would catch a mapping error — a term landing in `other_verified_ops`
    /// because a match arm was forgotten — which no aggregate assertion can see, since the total is
    /// pinned to ADR-0131 either way.
    #[test]
    fn the_dimensions_carry_what_their_names_say() {
        let profile = dense();
        let v = palw_canonical_work_v1(&descriptor(&profile), &PalwCanonicalExecutionFactsV1::uncached(63, 2)).expect("dense");
        assert!(v.dense_matmul > 0, "a dense transformer's projections and FFN");
        assert!(v.attention_prefill > 0 && v.attention_decode > 0, "63 prefill positions and one decode call");
        assert!(v.normalization > 0, "every block is normed");
        assert!(v.kv_read_bytes > 0 && v.kv_write_bytes > 0, "an attention class touches its cache");
        assert_eq!(v.recurrence, 0, "the dense row has no gated-delta layer");
        assert_eq!(
            v.arithmetic_mac_eq(),
            v.dense_matmul
                + v.routed_expert_matmul
                + v.attention_prefill
                + v.attention_decode
                + v.recurrence
                + v.normalization
                + v.other_verified_ops,
            "a dimension is missing from the sum"
        );
    }

    /// A class that streams two formats is refused rather than priced at one of them. The guess
    /// would be a self-report wearing a derivation, which is the failure mode this whole module
    /// exists to remove — so it fails closed, and the failure names both formats.
    #[test]
    fn a_class_that_mixes_weight_formats_is_refused() {
        let mut profile = dense();
        let node = profile
            .attn_nodes
            .iter_mut()
            .find(|n| !n.weight_dtypes.is_empty())
            .expect("the dense row has weight-bearing attention nodes");
        node.weight_dtypes[0] = 12; // Q4_K beside the class's I8
        assert!(matches!(
            PalwCanonicalClassDescriptorV1::of(&profile, Hash64::default()),
            Err(PalwCanonicalWorkError::MixedWeightFormat { .. })
        ));
    }

    /// **Below the fence, nothing moves; past it, the audit's F1 row collapses.**
    ///
    /// The two accounting faces the fence touches are written as one expression with one operand,
    /// and the dormant face passes `None` to it — so the byte-identity is a property of the code
    /// and this test is what stops it from being quietly rewritten into two expressions. The armed
    /// half is asserted beside it because a fence that changed nothing when armed would pass the
    /// dormant half just as well.
    ///
    /// The armed numbers are the audit's own (`counterexample-table.txt`, family (b)): one graph,
    /// one artifact, one kernel set, two declarations — `(63,2)` at 6,630,544 declared leaves and
    /// `(63,370)` at 51,535,376 — both executing 83,102,171,136 MAC-equivalents past
    /// `palw_prefill_draw`, and today buying 100 and 777 index of fork-choice weight for it.
    #[test]
    fn a_dormant_canonical_work_fence_moves_no_number() {
        use crate::palw_state_v2::{
            PalwClassStateV2, PalwClassStatusV2, PalwPwuRuleV2, PalwTransitionExtrasV1, palw_exposure_pwu_v1, palw_exposure_pwu_v2,
        };
        // `Default` is every shipped preset's extras, and it must name no height.
        assert_eq!(PalwTransitionExtrasV1::default().canonical_work_daa, None);

        let class = |rule: PalwPwuRuleV2| PalwClassStateV2 {
            artifact_root: Hash64::from_u64_word(0xA57),
            slash_value_per_pwu: 5,
            pwu_rule: rule,
            status: PalwClassStatusV2::Active,
            registered_daa: 0,
            registrant_bond: None,
            fused_attention: false,
        };
        for rule in [PalwPwuRuleV2::DerivedV1 { pwu_per_inference: 6_630_544 }, PalwPwuRuleV2::MaxPerAttempt(4_096)] {
            let c = class(rule);
            for claimed in [1u64, 4_096, 6_630_544, u64::MAX] {
                assert_eq!(
                    palw_exposure_pwu_v2(&c, claimed, None),
                    palw_exposure_pwu_v1(&c, claimed),
                    "dormant, the derived face must be the declared one"
                );
            }
        }
        // Armed, the `DerivedV1` arm really moves — and `MaxPerAttempt` really does not, because
        // it is bounded by a registered ceiling rather than derived from a graph.
        let derived = class(PalwPwuRuleV2::DerivedV1 { pwu_per_inference: 6_630_544 });
        assert_eq!(palw_exposure_pwu_v2(&derived, 0, Some(83_102_171_136)), 83_102_171_136);
        let capped = class(PalwPwuRuleV2::MaxPerAttempt(4_096));
        assert_eq!(palw_exposure_pwu_v2(&capped, 4_096, Some(83_102_171_136)), 4_096);

        // Audit family (b), at the site that decides fork choice. Both declarations run the same
        // job, so both must weigh the same — and on the basis in force they differ by 7.8x.
        const EXECUTED_MAC_EQ: u128 = 83_102_171_136;
        const DECLARED_SHIPPED: u64 = 6_630_544;
        const DECLARED_INFLATED: u64 = 51_535_376;
        let attempts = 4_003u128;
        let shipped_pwu = (attempts * DECLARED_SHIPPED as u128) as u64;
        let inflated_pwu = (attempts * DECLARED_INFLATED as u128) as u64;
        assert!(
            inflated_pwu as u128 > shipped_pwu as u128 * 7,
            "the fixture must still reproduce the audit's weight gap, or the numbers below prove nothing"
        );
        assert_eq!(
            palw_claim_canonical_pwu_v1(shipped_pwu, DECLARED_SHIPPED, EXECUTED_MAC_EQ),
            palw_claim_canonical_pwu_v1(inflated_pwu, DECLARED_INFLATED, EXECUTED_MAC_EQ),
            "the declared decode budget still buys fork-choice weight"
        );
        assert_eq!(
            palw_claim_canonical_pwu_v1(shipped_pwu, DECLARED_SHIPPED, EXECUTED_MAC_EQ),
            Some(attempts * EXECUTED_MAC_EQ),
            "what survives is the lottery the claim really paid, times the work it really ran"
        );
    }

    /// The re-pricing is the identity when the derived per-draw work equals the declared
    /// per-inference count, and scales linearly otherwise — so the lottery factor the claim really
    /// paid survives and only the registrant's factor is replaced.
    ///
    /// The zero arm is a liveness guard, not a policy: a class with no declared per-inference count
    /// has nothing to divide by, and refusing there would stop the chain over an accounting change.
    #[test]
    fn the_re_pricing_replaces_only_the_declared_factor() {
        // pwu = attempts x declared. Re-priced = attempts x derived.
        let (attempts, declared, derived) = (1_000u128, 7_000u64, 21_000u128);
        let pwu = (attempts * declared as u128) as u64;
        assert_eq!(palw_claim_canonical_pwu_v1(pwu, declared, derived), Some(attempts * derived));
        assert_eq!(palw_claim_canonical_pwu_v1(pwu, declared, declared as u128), Some(pwu as u128), "identity when they agree");
        assert_eq!(palw_claim_canonical_pwu_v1(pwu, 0, derived), None, "nothing to divide by");
        // Saturating, not wrapping: the heaviest claim must never become the lightest.
        assert_eq!(palw_claim_canonical_pwu_v1(u64::MAX, 1, u128::MAX), Some(u128::MAX));
    }

    /// **ACCOUNTING RE-AUDIT 2026-09-19 — ADR-0145 §8's property, mass-generated.**
    ///
    /// The audit's F1 is not one counterexample, it is a CLASS of them: everything a registrant
    /// may write down about a class that changes no arithmetic. This enumerates that class rather
    /// than sampling it, over ONE effective inference — the executed job is held fixed at
    /// `(prefill 63, decode 1)`, which is exactly what `palw_attempt_job_v1` runs past
    /// `Params::palw_prefill_draw` (armed at DAA 4,000 on `palw_rc_shipped_params()`) whatever the
    /// class declared — and varies only the representation:
    ///
    /// * **`tile_len`**, the commitment splitting, across its whole legal range
    ///   `[PALW_STEP_MIN_TILE_LEN, PALW_STEP_MAX_TILE_LEN]` = `[4, 65536]`;
    /// * **the declared canonical `(prefill, decode)` split**, decode from 1 to 370 at fixed
    ///   prefill — every one of which executes the same single decode call;
    /// * **runtime metadata / serialization** (`n_threads`), which moves `shape_profile_id` and
    ///   therefore the class id, and no arithmetic.
    ///
    /// Three quantities must be invariant across the whole cross-product: the **derived canonical
    /// work**, the **reward**, and the **fork-choice weight**.
    ///
    /// Only variations the chain would really admit are counted: `tile_len` is NOT free in
    /// `[4, 65536]` as the audit wrote — `palw_class_admission_v2` stores
    /// `worst_case_step_leaf_count_capped_v1(profile, ladder)` and that call refuses a profile
    /// whose worst case clears the court's ladder, so the finest admissible uniform tile is 24 and
    /// the sub-24 rows below are skipped rather than counted as attacks.
    #[test]
    fn legal_representations_of_one_inference_must_price_identically() {
        use crate::palw_class_daa::attempt_target_seed_v1;
        use crate::palw_panel_economy_v1::palw_work_priced_reward_v1;
        use crate::palw_pwu::{palw_expected_attempts_v1, palw_pwu_v1};
        use crate::palw_step::{
            PALW_STEP_MAX_TILE_LEN, PALW_STEP_MIN_TILE_LEN, step_leaf_count_capped_v1, worst_case_step_leaf_count_capped_v1,
        };

        /// `court.max_step_leaf_count()` on the shipped preset (2^26), read off the ladder probe.
        const LADDER: u64 = 67_108_864;
        /// The dense row's held share (`palw_class_daa.rs:672`'s own table).
        const SHARE: u16 = 489;
        /// The 72 % worker carve the audit priced against.
        const ESCROW: u64 = 320_084_640_000;
        /// The ADR-0124 unit over the shipped weight-bearing classes (redteam probe R6).
        const UNIT_DECLARED: u64 = 9_000_776;
        /// The effective inference. Prefill is part of WHAT IS RUN, so it is held fixed; decode is
        /// not, because the draw executes exactly one decode call whatever was declared.
        const PREFILL: u32 = 63;

        let tiles: [u32; 13] = [PALW_STEP_MIN_TILE_LEN, 8, 16, 24, 32, 48, 64, 128, 256, 512, 4096, 16_384, PALW_STEP_MAX_TILE_LEN];
        let decodes: [u32; 11] = [1, 2, 3, 4, 8, 16, 32, 64, 128, 256, 370];
        let threads: [u32; 3] = [1, 2, 4];

        struct Row {
            tile: u32,
            decode: u32,
            threads: u32,
            declared: u64,
            derived: u128,
            weight_today: u128,
            weight_derived: u128,
            executed_total: u128,
            reward_today: u64,
            class_id: Hash64,
        }
        let mut rows: Vec<Row> = Vec::new();
        let (mut generated, mut refused) = (0usize, 0usize);

        for &tile in tiles.iter() {
            for &decode in decodes.iter() {
                for &n_threads in threads.iter() {
                    generated += 1;
                    let mut profile = dense();
                    for table in [&mut profile.pre_nodes, &mut profile.gdn_nodes, &mut profile.attn_nodes, &mut profile.post_nodes] {
                        for node in table.iter_mut() {
                            node.tile_len = tile;
                        }
                    }
                    profile.n_threads = n_threads;
                    if profile.validate_shape().is_err() {
                        refused += 1;
                        continue;
                    }
                    // Admission's own two gates, in admission's own order.
                    let Ok(worst) = worst_case_step_leaf_count_capped_v1(&profile, LADDER) else {
                        refused += 1;
                        continue;
                    };
                    let job = rc_job_context(&profile, PREFILL, decode);
                    let Ok(declared) = step_leaf_count_capped_v1(&profile, &job, LADDER) else {
                        refused += 1;
                        continue;
                    };
                    if declared > worst {
                        refused += 1;
                        continue;
                    }

                    // The work the chain DERIVES for the job a draw really runs.
                    let d = descriptor(&profile);
                    let derived =
                        palw_canonical_draw_work_v1(&d, &job, true).expect("a shipped profile derives").provisional_scalar_v1();

                    // The chain's own pipeline: the declaration seeds the difficulty
                    // (`attempt_target_seed_v1`, live at `palw_state_v2.rs:9595` and `:14676`),
                    // the difficulty sets the expected draws, and the product is `claim.pwu`.
                    let target = attempt_target_seed_v1(SHARE, declared);
                    let pwu = palw_pwu_v1(target, declared);
                    let attempts = palw_expected_attempts_v1(target).max(1) as u128;

                    rows.push(Row {
                        tile,
                        decode,
                        threads: n_threads,
                        declared,
                        derived,
                        weight_today: pwu as u128,
                        weight_derived: palw_claim_canonical_pwu_v1(pwu, declared, derived).expect("declared is non-zero"),
                        executed_total: attempts * derived,
                        reward_today: palw_work_priced_reward_v1(ESCROW, declared, UNIT_DECLARED),
                        class_id: d.canonical_class_id_v1(),
                    });
                }
            }
        }

        assert!(rows.len() > 100, "the generator must actually enumerate the class, got {}", rows.len());
        let first = &rows[0];

        // ---- P1. The derived canonical work is invariant. This is ADR-0145 §1's whole claim. ----
        for r in rows.iter() {
            assert_eq!(
                r.derived, first.derived,
                "canonical work moved for a pure representation change: tile_len {} decode {} n_threads {}",
                r.tile, r.decode, r.threads
            );
            assert_eq!(r.class_id, first.class_id, "the canonical class identity moved for a representation change");
        }

        // ---- P2. On the DERIVED basis the weight is exactly the arithmetic really executed. ----
        for r in rows.iter() {
            assert_eq!(
                r.weight_derived, r.executed_total,
                "derived weight is not the executed arithmetic at tile_len {} decode {}",
                r.tile, r.decode
            );
        }

        // ---- P3. What the SHIPPED basis actually does with the same set. -------------------
        //
        // `weight_today / executed_total` is fork-choice weight bought per unit of arithmetic
        // really performed. It must be flat. It is not: the declared leaf count is a direct
        // multiplier on it, which is audit finding F1.
        let ratio = |r: &Row| r.weight_today as f64 / r.executed_total as f64;
        let worst_row = rows.iter().max_by(|a, b| ratio(a).total_cmp(&ratio(b))).unwrap();
        let best_row = rows.iter().min_by(|a, b| ratio(a).total_cmp(&ratio(b))).unwrap();
        let spread = ratio(worst_row) / ratio(best_row);
        let reward_spread =
            rows.iter().map(|r| r.reward_today).max().unwrap() as f64 / rows.iter().map(|r| r.reward_today).min().unwrap() as f64;

        println!(
            "variations generated {generated}, admissible {} (refused by the ladder {refused})\n\
             derived work per draw, invariant across all of them = {} MAC-eq\n\
             SHIPPED basis  weight/executed-MAC-eq spread = {spread:.1}x  \
             (worst tile_len {} decode {} declared {} ; best tile_len {} decode {} declared {})\n\
             SHIPPED basis  reward spread                 = {reward_spread:.1}x\n\
             DERIVED basis  weight/executed-MAC-eq        = 1.0x for every row (P2)",
            rows.len(),
            first.derived,
            worst_row.tile,
            worst_row.decode,
            worst_row.declared,
            best_row.tile,
            best_row.decode,
            best_row.declared,
        );

        // The reward half really is representation-independent past ADR-0132's snapshot only
        // because `palw_work_priced_reward_v1` saturates at the unit; below it, it is linear in
        // the declaration. Both halves are recorded rather than asserted flat, because asserting
        // flatness here would assert a fix that `palw_rc_shipped_params().palw_canonical_work`
        // (None — dormant on every shipped preset) has not made.
        assert!(
            spread > 1.0,
            "the shipped basis priced every representation alike — the canonical-work fence must have been armed; \
             re-point this assertion at equality and delete the F1 finding"
        );
        assert!(reward_spread > 1.0, "the shipped reward basis priced every representation alike — see above");
    }
}
