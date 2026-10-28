//! **Every court arm, fed by a hostile producer and a hostile registrant, answers — it never
//! panics.**
//!
//! The one-move court recomputes a disputed step inside block processing (`adjudicate_court_close_v3`
//! from the virtual processor), and the release profile keeps `overflow-checks = true`. A panic on
//! this path is therefore not a refused dispute: the block is stored and relayed before the node
//! dies, and every node that processes it — and re-processes it on restart — dies the same way.
//! A remote halt of every validating node.
//!
//! Two parties the court does not trust choose what the arms read:
//!
//! * the PRODUCER, whose committed lanes arrive as raw `i32` bit patterns — any of `2^32`, whatever
//!   the node that wrote them could honestly have produced;
//! * the REGISTRANT, whose artifact bytes the oracle serves — tables, parameter triples, weight
//!   codes, exponents, taps — any bytes the class's inventory root commits to.
//!
//! This walks the dispatch the court runs ([`run_program`]) at every node of every profile family
//! the tree builds — the A16 dense graphs, the Qwen3.6 hybrid graphs, BASE-0, the Kimi K3 graph
//! (fenced but compiled), the float GDN wiring — at prefill and decode coordinates and every tile
//! the lane-sliced arms slice, with both parties at their extremes, and then sweeps every
//! catalogued kernel once more through a synthetic node at odd widths. The only admissible
//! outcomes are a recomputed row or a refusal.
//!
//! **What "fixed" has to mean here.** The court runs on every node, so a fix that changes the
//! answer for an input which did NOT panic before would be a consensus change — an old and a new
//! binary disagreeing about a block. The fixes this test pins change behaviour only where the old
//! code panicked, which is what lets them ship as a node update with no fence.

use super::*;
use crate::palw_step::{PalwStepNodeV1, PalwStepOutLenV1};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};

// ---------------------------------------------------------------------------------------------
// Catching a panic without drowning the log in them
// ---------------------------------------------------------------------------------------------

thread_local! {
    static QUIET: Cell<bool> = const { Cell::new(false) };
    static LAST_PANIC: RefCell<String> = const { RefCell::new(String::new()) };
}

/// A process-wide hook that records the panic of a `guarded` call on its own thread and defers to
/// the previous hook everywhere else — so the tests running beside this one keep their messages.
fn install_quiet_hook() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if QUIET.with(Cell::get) {
                LAST_PANIC.with(|l| *l.borrow_mut() = info.to_string().replace('\n', " "));
            } else {
                previous(info);
            }
        }));
    });
}

/// Run `f`; a panic comes back as `Err("panicked at file:line:col: message")`.
pub(crate) fn guarded<T>(f: impl FnOnce() -> T) -> Result<T, String> {
    install_quiet_hook();
    QUIET.with(|q| q.set(true));
    let r = catch_unwind(AssertUnwindSafe(f));
    QUIET.with(|q| q.set(false));
    r.map_err(|_| LAST_PANIC.with(|l| l.borrow().clone()))
}

fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn name_hash(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100_0000_01b3))
}

// ---------------------------------------------------------------------------------------------
// The registrant: any bytes, at any offset, of exactly the length asked for
// ---------------------------------------------------------------------------------------------

/// What a hostile registrant's artifact holds. Every byte is a function of `(name, layer, absolute
/// offset)`, so a table read in two pieces reads one table, and a 17-byte triple store lines its
/// fields up with the triples the arms decode.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Artifact {
    Fill(u8),
    Noise(u64),
    /// Every byte one of `00 FF 80 7F 01 FE`.
    Extreme(u64),
    /// Every 17 bytes one `A16QuantParams` wire triple `(multiplier, shift, zero)`.
    Triple(i64, u8, i64),
    /// Every 4 bytes one little-endian `i32` — the rotation tables' word.
    Word(i32),
}

pub(crate) struct Registrant(pub(crate) Artifact);

impl PalwWeightOracleV1 for Registrant {
    fn operand_bytes(&self, name: &str, layer: Option<u16>, byte_offset: u32, byte_len: u32) -> Option<Vec<u8>> {
        // A real oracle serves what the inventory commits; nothing a court asks for is this large,
        // and answering would only measure the allocator.
        if byte_len > 1 << 22 {
            return None;
        }
        let base = name_hash(name) ^ (layer.map_or(0xFFFF, u64::from) << 48);
        Some(
            (0..byte_len as u64)
                .map(|i| {
                    let pos = byte_offset as u64 + i;
                    match self.0 {
                        Artifact::Fill(b) => b,
                        Artifact::Noise(s) => mix(s ^ base ^ pos) as u8,
                        Artifact::Extreme(s) => [0x00, 0xFF, 0x80, 0x7F, 0x01, 0xFE][(mix(s ^ base ^ pos) % 6) as usize],
                        Artifact::Triple(m, shift, z) => match (pos % 17) as usize {
                            u @ 0..8 => m.to_le_bytes()[u],
                            8 => shift,
                            u => z.to_le_bytes()[u - 9],
                        },
                        Artifact::Word(w) => w.to_le_bytes()[(pos % 4) as usize],
                    }
                })
                .collect(),
        )
    }
}

pub(crate) fn registrants() -> Vec<Artifact> {
    let mut v = vec![Artifact::Fill(0), Artifact::Fill(0xFF), Artifact::Fill(0x80), Artifact::Fill(0x7F), Artifact::Fill(1)];
    for s in 0..3 {
        v.push(Artifact::Noise(s));
        v.push(Artifact::Extreme(s));
    }
    for (m, shift, z) in [
        (i64::MIN, 62, i64::MIN),
        (i64::MAX, 62, i64::MAX),
        (i64::MAX, 0, i64::MAX),
        (i64::MIN, 0, i64::MIN),
        (i64::MIN, 0, i64::MAX),
        (i64::MAX, 0, i64::MIN),
        (1, 0, i64::MAX),
        (-1, 0, i64::MIN),
        (i64::MAX, 31, 0),
        (1, 0, 0),
        (1, 24, 62),
        (1 << 40, 24, 1 << 40),
        // One past the triple's shift domain: the decoder's refusal, not the arithmetic, answers.
        (1, 63, 0),
    ] {
        v.push(Artifact::Triple(m, shift, z));
    }
    for w in [i32::MIN, i32::MAX, -1, 1 << 24, -(1 << 24), 1 << 30] {
        v.push(Artifact::Word(w));
    }
    v
}

// ---------------------------------------------------------------------------------------------
// The producer: any committed lanes
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub(crate) enum Lanes {
    Fill(i32),
    /// `i32::MIN, i32::MAX, …`
    Alternate,
    Noise(u64),
    /// Every lane one of the values an arm's bounds are written against.
    Extreme(u64),
    /// `0, 1, 2, 3, …` — expert ids a routing row can name, so the routed arms get past the ids.
    Small,
}

const EXTREME_LANES: [i32; 13] =
    [i32::MIN, i32::MAX, 0, -1, 1, 32_767, -32_767, 32_768, -32_768, 1 << 24, -(1 << 24), 1 << 30, -(1 << 30)];

pub(crate) fn lanes(p: Lanes, len: usize, row: usize) -> Vec<u32> {
    (0..len)
        .map(|i| {
            let salt = ((row as u64) << 32) ^ i as u64;
            (match p {
                Lanes::Fill(v) => v,
                Lanes::Alternate => {
                    if i % 2 == 0 {
                        i32::MIN
                    } else {
                        i32::MAX
                    }
                }
                Lanes::Noise(s) => mix(s ^ salt) as i32,
                Lanes::Extreme(s) => EXTREME_LANES[(mix(s ^ salt) % EXTREME_LANES.len() as u64) as usize],
                Lanes::Small => (i % 4) as i32,
            }) as u32
        })
        .collect()
}

pub(crate) fn producers() -> Vec<Lanes> {
    vec![
        Lanes::Fill(i32::MIN),
        Lanes::Fill(i32::MAX),
        Lanes::Fill(-32_768),
        Lanes::Fill(32_767),
        Lanes::Fill(0),
        Lanes::Fill(-1),
        Lanes::Alternate,
        Lanes::Noise(1),
        Lanes::Extreme(1),
        Lanes::Extreme(2),
        Lanes::Small,
    ]
}

// ---------------------------------------------------------------------------------------------
// The profiles: every family the tree builds, at a geometry small enough to sweep
// ---------------------------------------------------------------------------------------------

fn families() -> Vec<(String, PalwShapeProfileV3)> {
    use crate::palw_qwen25_profile as q25;
    use crate::palw_qwen36_profile as q36;
    let mut out: Vec<(String, PalwShapeProfileV3)> = Vec::new();
    let mut push = |name: String, p: Result<PalwShapeProfileV3, crate::palw_step::PalwStepError>| {
        if let Ok(p) = p {
            out.push((name, p));
        }
    };

    // The hybrid, at the fuzz fixture's geometry — one tile per row, and a head-sized tile so the
    // lane- and head-sliced arms slice.
    for tile_len in [512u32, 8] {
        for gate in [1u8, 0] {
            let g = q36::PalwQwen36GeometryV1 {
                layer_count: 4,
                full_attention_interval: 4,
                hidden_dim: 32,
                attn_heads: 4,
                attn_kv_heads: 2,
                attn_head_dim: 16,
                rope_dims: 4,
                rope_freq_base_bits: 0x4B18_9680,
                gdn_k_heads: 2,
                gdn_v_heads: 4,
                gdn_head_dim: 8,
                gdn_conv_kernel: 4,
                n_experts: 8,
                experts_per_token: 4,
                moe_dim: 16,
                shared_dim: 16,
                attn_output_gate: gate,
                vocab_size: 64,
                n_ctx: 8,
                n_threads: 1,
                rms_eps_q: 1,
                tile_len,
            };
            let tag = format!("t{tile_len}/gate{gate}");
            push(format!("q36-v1/{tag}"), q36::qwen36_profile_v1(g));
            push(format!("q36-v2/{tag}"), q36::qwen36_profile_v2(g));
            push(format!("q36-v5/{tag}"), q36::qwen36_profile_v5(g));
            push(format!("q36-v6/{tag}"), q36::qwen36_profile_v6(g));
            push(format!("q36-v7/{tag}"), q36::qwen36_profile_v7(g));
            push(format!("q36-row5/{tag}"), q36::qwen36_artifact_row_profile_v5(g));
            push(format!("q36-row6/{tag}"), q36::qwen36_artifact_row_profile_v6(g));
        }
    }

    // The dense A16 family and its BASE-0 projection.
    for tile_len in [32u32, 8] {
        let g = q25::PalwQwen25GeometryV1 {
            layer_count: 2,
            hidden_dim: 64,
            ffn_dim: 128,
            attn_heads: 2,
            attn_kv_heads: 1,
            attn_head_dim: 32,
            vocab_size: 64,
            n_ctx: 8,
            n_threads: 1,
            rms_eps_q: 1,
            tile_len,
        };
        let tag = format!("t{tile_len}");
        push(format!("q25-a16-v1/{tag}"), q25::qwen25_a16_profile_v1(g));
        push(format!("q25-a16-v2/{tag}"), q25::qwen25_a16_profile_v2(g));
        push(format!("q25-a16-v5/{tag}"), q25::qwen25_a16_profile_v5(g));
        push(format!("q25-a16-v7/{tag}"), q25::qwen25_a16_profile_v7(g));
        push(format!("q25-a16-row1/{tag}"), q25::qwen25_a16_artifact_row_profile_v1(g));
        push(format!("q25-a16-row5/{tag}"), q25::qwen25_a16_artifact_row_profile_v5(g));
        push(format!("q25-a16-row7/{tag}"), q25::qwen25_a16_artifact_row_profile_v7(g));
        push(format!("q25-base0/{tag}"), q25::qwen25_profile_v1(g));
    }

    // BASE-0 at a sweepable width.
    push(
        "base0".into(),
        crate::palw_base0_profile::base0_profile_v1(crate::palw_base0_profile::PalwBase0GeometryV1 {
            layer_count: 2,
            hidden_dim: 32,
            ffn_dim: 64,
            attn_heads: 2,
            attn_head_dim: 16,
            vocab_size: 64,
            n_ctx: 8,
            n_threads: 1,
            rms_eps_q: 1,
            tile_len: 8,
        }),
    );

    // Kimi K3: the card's layer schedule (the profile refuses any other), every width shrunk.
    push(
        "kimi-k3".into(),
        crate::palw_kimi_k3_profile::kimi_k3_profile_v1(crate::palw_kimi_k3_profile::PalwKimiK3GeometryV1 {
            hidden_dim: 32,
            attn_heads: 2,
            attn_head_dim: 16,
            qk_rope_head_dim: 4,
            kv_lora_rank: 8,
            v_head_dim: 16,
            kda_heads: 2,
            kda_head_dim: 8,
            n_experts: 8,
            experts_per_token: 2,
            shared_experts: 1,
            moe_dim: 16,
            vocab_size: 64,
            n_ctx: 10,
            tile_len: 8,
            ..crate::palw_kimi_k3_profile::KIMI_K3_CARD
        }),
    );

    // The float lane's GDN wiring — the court's own fixture — and its AVX2 twin, whose dot steps
    // 32 lanes, so its head's k width is 32 and the q/k rows the recurrence reads are two heads of
    // it.
    out.push(("float-gdn".into(), super::tests::profile()));
    let mut avx2 = super::tests::profile();
    avx2.gdn_head_k_dim = 32;
    for (i, node) in avx2.gdn_nodes.iter_mut().enumerate() {
        match i {
            0 | 1 => node.out_len = PalwStepOutLenV1::Fixed { elements: 64 },
            5 => node.kernel_semantics_id = kernel_semantics_id_v1(KDESC_GDN_CORE_AVX2),
            _ => {}
        }
    }
    out.push(("float-gdn-avx2".into(), avx2));
    out
}

// ---------------------------------------------------------------------------------------------
// The sweep
// ---------------------------------------------------------------------------------------------

/// A leaf's canonical width — `PalwStepLegBuilderV1::canonical_tile_values`, restated because a
/// test must not import the thing it checks the court against (and that one is private).
fn tile_values(
    profile: &PalwShapeProfileV3,
    ctx: &crate::palw_v2::PalwJobContextV2,
    c: &PalwStepCoordinateV1,
) -> Option<(usize, u64)> {
    let (node, _) = profile.resolve_node_slot(c.node_slot)?;
    let kv_len = if c.call_index == 0 { c.position as u64 + 1 } else { ctx.declared_prefill_tokens as u64 + c.call_index as u64 };
    let len = match node.out_len {
        PalwStepOutLenV1::Fixed { elements } => elements as u64,
        PalwStepOutLenV1::KvScaled { multiplier } => multiplier as u64 * kv_len,
    };
    let tile = node.tile_len.max(1) as u64;
    let start = c.tile_index as u64 * tile;
    (start < len).then(|| ((len - start).min(tile) as usize, len.div_ceil(tile)))
}

#[derive(Default)]
struct Tally {
    /// Per program: calls that recomputed a row, calls refused, calls that panicked.
    per_program: BTreeMap<String, (u64, u64, u64)>,
    /// First example of each distinct panic site.
    panics: BTreeMap<String, String>,
}

impl Tally {
    fn record(
        &mut self,
        program: KernelProgram,
        context: impl FnOnce() -> String,
        r: Result<Result<(Vec<u32>, usize), PalwStepRefuteError>, String>,
    ) {
        let e = self.per_program.entry(format!("{program:?}")).or_default();
        match r {
            Ok(Ok(_)) => e.0 += 1,
            Ok(Err(_)) => e.1 += 1,
            Err(panic) => {
                e.2 += 1;
                let site = panic.split(": ").next().unwrap_or(&panic).to_string();
                self.panics.entry(site).or_insert_with(|| format!("{panic}\n      reached by {}", context()));
            }
        }
    }

    fn report(&self) -> String {
        let mut s = String::new();
        for (site, example) in &self.panics {
            s.push_str(&format!("\n  PANIC {site}\n      {example}"));
        }
        s
    }
}

fn is_gather(program: KernelProgram) -> bool {
    matches!(
        program,
        KernelProgram::Base0(Base0Op::Embed)
            | KernelProgram::Qwen36(Qwen36Op::Embed)
            | KernelProgram::Qwen36(Qwen36Op::RequantizeByToken)
    )
}

fn sweep_profile(name: &str, profile: &PalwShapeProfileV3, tally: &mut Tally) {
    let (prefill, decode) = (3u32, 2u32);
    let ctx = crate::palw_base0_profile::rc_job_context(profile, prefill, decode);
    let vocab = profile.vocab_size;
    let token_sets: [Vec<u32>; 4] = [vec![1, 2, 3], vec![u32::MAX; 3], vec![vocab; 3], vec![1 << 31; 3]];
    // One representative of each node of each table kind — the first two layers of a kind carry
    // every wiring the rest repeat (a layer's `LAYER_IN` is the previous table's terminal node).
    let mut seen: BTreeMap<(String, usize), u32> = BTreeMap::new();
    for slot in 0..profile.global_node_count() {
        let Some((node, layer)) = profile.resolve_node_slot(slot) else { continue };
        let Some(program) = resolve_kernel(&node.kernel_semantics_id) else { continue };
        let table = match layer {
            _ if (slot as usize) < profile.pre_nodes.len() => "pre".to_string(),
            Some(l) => format!("{:?}", profile.layer_kind(l)),
            None => "post".to_string(),
        };
        let Some(intra) = intra_table_index(profile, slot) else { continue };
        let n = seen.entry((table, intra)).or_insert(0);
        *n += 1;
        if *n > 2 {
            continue;
        }
        let coords = [(0u32, 0u32), (0, prefill - 1), (1, 0), (decode, 0)];
        for (call_index, position) in coords {
            let first = PalwStepCoordinateV1 { call_index, node_slot: slot, position, tile_index: 0 };
            let Some((_, tiles)) = tile_values(profile, &ctx, &first) else { continue };
            let mut tile_indices = vec![0u64, 1, tiles.saturating_sub(1)];
            tile_indices.retain(|t| *t < tiles);
            tile_indices.dedup();
            for tile_index in tile_indices {
                let coord = PalwStepCoordinateV1 { tile_index: tile_index as u32, ..first };
                let Some(required) = canonical_input_leaves_v1(profile, &ctx, &coord) else { continue };
                let widths: Option<Vec<usize>> =
                    required.iter().map(|row| row.iter().map(|(_, c)| tile_values(profile, &ctx, c).map(|(w, _)| w)).sum()).collect();
                let Some(widths) = widths else { continue };
                let kv_len = if call_index == 0 { position as u64 + 1 } else { prefill as u64 + call_index as u64 };
                let routing = qwen36_reads_routing_v1(node);
                let token_choices: &[Vec<u32>] = if is_gather(program) { &token_sets } else { &token_sets[..1] };
                for tokens in token_choices {
                    let window = crate::palw_prompt_ids_v1::PalwPromptIdWindowV1::whole_checked(tokens);
                    let generated = tokens.clone();
                    for producer in producers() {
                        // A routing reader's last row is `[ids…, weights…]`: once with ids the
                        // router could name (so the arithmetic behind them is reached), once raw.
                        for small_ids in if routing { &[false, true][..] } else { &[false][..] } {
                            let inputs: Vec<Vec<u32>> = widths
                                .iter()
                                .enumerate()
                                .map(|(r, &w)| {
                                    let mut row = lanes(producer, w, r);
                                    if *small_ids && r + 1 == widths.len() {
                                        for (i, lane) in row.iter_mut().take(w / 2).enumerate() {
                                            *lane = (i % 4) as u32;
                                        }
                                    }
                                    row
                                })
                                .collect();
                            for artifact in registrants() {
                                let oracle = Registrant(artifact);
                                let r = guarded(|| {
                                    run_program(program, node, layer, profile, &inputs, &oracle, kv_len, (&coord, window, &generated))
                                });
                                tally.record(
                                    program,
                                    || format!("{name} slot {slot} {coord:?} producer {producer:?} registrant {artifact:?} tokens {tokens:?}"),
                                    r,
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Every catalogued kernel — the fenced ones too — through a node the profile does not hold, at
/// widths no honest wiring produces: empty, one lane, odd, mismatched between operands.
fn sweep_synthetic(tally: &mut Tally) {
    let profile = super::tests::profile();
    let all = KERNEL_CATALOG.iter().chain(KERNEL_CATALOG_FENCED_V1).chain(KERNEL_CATALOG_FENCED_KIMI_V1);
    for (desc, program) in all {
        for weight_name in [
            "",
            "blk.{layer}.w",
            "blk.{layer}.ffn_expert_gated.a16",
            "blk.{layer}.linear_decay.a16",
            "blk.{layer}.ffn_gate_exps.routed",
        ] {
            for (arity, width, second) in
                [(1usize, 0usize, 0usize), (1, 1, 1), (2, 7, 7), (2, 16, 3), (3, 32, 32), (5, 16, 16), (10, 32, 32)]
            {
                for tile_len in [1u32, 16] {
                    let node = PalwStepNodeV1 {
                        op_kind: crate::palw_step::PalwStepOpKindV1::MulElem,
                        role: crate::palw_step::PalwStepNodeRoleV1::Plain,
                        weight_name: weight_name.to_string(),
                        weight_dtypes: Vec::new(),
                        out_len: PalwStepOutLenV1::Fixed { elements: width as u32 },
                        tile_len,
                        kernel_semantics_id: kernel_semantics_id_v1(desc),
                        input_refs: (0..arity as u16).collect(),
                    };
                    for producer in producers() {
                        let inputs: Vec<Vec<u32>> =
                            (0..arity).map(|r| lanes(producer, if r == 0 { width } else { second }, r)).collect();
                        for artifact in registrants() {
                            let oracle = Registrant(artifact);
                            let coord = PalwStepCoordinateV1 { call_index: 0, node_slot: 1, position: 0, tile_index: 0 };
                            let ids = [1u32, 2, 3];
                            let window = crate::palw_prompt_ids_v1::PalwPromptIdWindowV1::whole_checked(&ids);
                            for kv_len in [1u64, 4] {
                                let r = guarded(|| {
                                    run_program(*program, &node, Some(0), &profile, &inputs, &oracle, kv_len, (&coord, window, &ids))
                                });
                                tally.record(
                                    *program,
                                    || format!("synthetic {desc} weight {weight_name:?} arity {arity} widths {width}/{second} tile {tile_len} producer {producer:?} registrant {artifact:?} kv_len {kv_len}"),
                                    r,
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    // Kimi's combine reads three rows — `k` routed blocks, the shared experts, the routing — whose
    // widths no generic shape lines up and the Kimi graph's own wiring does not either (its arms
    // are fenced and recorded as defective). Lined up here, so the arithmetic is reached.
    let shared = crate::palw_kimi_k3_ops::KIMI_K3_SHARED_EXPERTS;
    for (k, hidden) in [(1usize, 1usize), (2, 8), (8, 4)] {
        let node = PalwStepNodeV1 {
            op_kind: crate::palw_step::PalwStepOpKindV1::MulElem,
            role: crate::palw_step::PalwStepNodeRoleV1::Plain,
            weight_name: String::new(),
            weight_dtypes: Vec::new(),
            out_len: PalwStepOutLenV1::Fixed { elements: hidden as u32 },
            tile_len: hidden as u32,
            kernel_semantics_id: kernel_semantics_id_v1(KDESC_KIMI_MOE_COMBINE),
            input_refs: vec![0, 1, 2],
        };
        let program = KernelProgram::Kimi(KimiOp::MoeCombine);
        for producer in producers() {
            for ids in [Lanes::Small, producer] {
                let mut routing = lanes(ids, k, 2);
                routing.extend(lanes(producer, k, 3));
                let inputs = vec![lanes(producer, k * hidden, 0), lanes(producer, shared * hidden, 1), routing];
                let coord = PalwStepCoordinateV1 { call_index: 0, node_slot: 1, position: 0, tile_index: 0 };
                let r = guarded(|| {
                    run_program(
                        program,
                        &node,
                        Some(0),
                        &profile,
                        &inputs,
                        &Registrant(Artifact::Fill(0)),
                        1,
                        (&coord, crate::palw_prompt_ids_v1::PalwPromptIdWindowV1::EMPTY, &[]),
                    )
                });
                tally.record(program, || format!("kimi combine k {k} hidden {hidden} producer {producer:?} ids {ids:?}"), r);
            }
        }
    }
}

/// **The sweep.** Fails with every distinct panic site and the first input that reached it.
#[test]
fn no_court_arm_panics_on_hostile_lanes_or_hostile_artifacts() {
    let mut tally = Tally::default();
    let families = families();
    let names: Vec<&str> = families.iter().map(|(n, _)| n.as_str()).collect();
    for prefix in ["q36-v2", "q36-v5", "q36-v7", "q25-a16-v1", "q25-a16-v5", "q25-base0", "base0", "kimi-k3", "float-gdn"] {
        assert!(names.iter().any(|n| n.starts_with(prefix)), "the {prefix} family must build at the sweep geometry; built {names:?}");
    }
    for (name, profile) in &families {
        sweep_profile(name, profile, &mut tally);
    }
    sweep_synthetic(&mut tally);

    // Coverage: every catalogued program — fenced ones included — recomputed a row at least once,
    // so the sweep reached its arithmetic rather than only its refusals.
    let unreached: Vec<&str> = KERNEL_CATALOG
        .iter()
        .chain(KERNEL_CATALOG_FENCED_V1)
        .chain(KERNEL_CATALOG_FENCED_KIMI_V1)
        .map(|(desc, program)| (*desc, format!("{program:?}")))
        .filter(|(_, p)| tally.per_program.get(p).is_none_or(|(ok, _, _)| *ok == 0))
        .map(|(desc, _)| desc)
        .collect();
    let summary: String = tally
        .per_program
        .iter()
        .map(|(p, (ok, err, panic))| format!("\n  {p:<48} ok {ok:>8}  refused {err:>8}  panicked {panic:>6}"))
        .collect();
    assert!(tally.panics.is_empty(), "court arms panicked on hostile input:{}\nper program:{summary}", tally.report());
    assert!(unreached.is_empty(), "the sweep never reached the arithmetic of {unreached:?}\nper program:{summary}");
}
