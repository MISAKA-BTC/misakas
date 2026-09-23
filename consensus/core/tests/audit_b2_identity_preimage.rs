//! **AUDIT B2 — what does the class identity actually commit to?**
//!
//! Read-only audit artefact. Nothing here is a fixture any shipped code reads.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::*;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_base0_profile::{PALW_RC_BASE0_CANONICAL, PALW_RC_BASE0_GEOMETRY, base0_profile_v1, rc_job_context};
use kaspa_consensus_core::palw_canonical_work_v1::PalwCanonicalClassDescriptorV1;
use kaspa_consensus_core::palw_economic_compute_v1::{PALW_ECONOMIC_COST_TABLE_V1, palw_attempt_economic_compute_v1};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_qwen25_profile::{QWEN25_1_5B, PalwQwen25GeometryV1, qwen25_a16_artifact_row_profile_v7, qwen25_a16_held_canonical_v1};
use kaspa_consensus_core::palw_qwen36_profile::{QWEN36_35B_A3B, PalwQwen36GeometryV1, qwen36_geometry_artifact_eps, qwen36_held_canonical_v1, qwen36_profile_v7};
use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;
use kaspa_consensus_core::palw_step::PalwShapeProfileV3;

fn t12_params() -> Params {
    Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12))
}

fn floor() -> PalwShapeProfileV3 {
    base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("floor")
}
fn hybrid() -> PalwShapeProfileV3 {
    qwen36_profile_v7(qwen36_geometry_artifact_eps(PalwQwen36GeometryV1 { n_ctx: PALW_T12_HYBRID_N_CTX, ..QWEN36_35B_A3B })).expect("hybrid")
}
fn dense() -> PalwShapeProfileV3 {
    qwen25_a16_artifact_row_profile_v7(PalwQwen25GeometryV1 { n_ctx: PALW_T12_DENSE_N_CTX, ..QWEN25_1_5B }).expect("dense")
}

fn canon_id(p: &PalwShapeProfileV3) -> Hash64 {
    PalwCanonicalClassDescriptorV1::of(p, Hash64::default()).expect("one weight format").canonical_class_id_v1()
}

/// The priced quantity: `economic_ccu_per_claim` = the MAC-eq of ONE draw (prefill + 1 token).
fn draw_ccu(p: &PalwShapeProfileV3, prefill: u32, decode: u32) -> u128 {
    let job = rc_job_context(p, prefill, decode);
    palw_attempt_economic_compute_v1(p, &job, true, &PALW_ECONOMIC_COST_TABLE_V1).expect("prices")
}

#[test]
fn b2_00_the_three_t12_rows_and_where_their_price_comes_from() {
    let p = t12_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!("t12 is ConsensusV2") };

    println!("\n=== t12 genesis ClassRegistered objects ===");
    for o in bundle.genesis_objects.iter() {
        if let PalwConsensusObjectV2::ClassRegistered { class_id, artifact_root, admission, share_permille, pwu_rule, .. } = o {
            println!(
                "class_id {class_id}\n  artifact_root {artifact_root}\n  share {share_permille}‰  carriage {}  pwu_rule {:?}",
                if admission.is_some() { "Some" } else { "None" },
                pwu_rule
            );
        }
    }

    let rows: [(&str, PalwShapeProfileV3, (u32, u32)); 3] = [
        ("BASE-0 floor", floor(), PALW_RC_BASE0_CANONICAL),
        ("Qwen3.6 v7@512", hybrid(), qwen36_held_canonical_v1(PALW_T12_HYBRID_N_CTX)),
        ("Qwen2.5 A16 v7@2M", dense(), qwen25_a16_held_canonical_v1(PALW_T12_DENSE_N_CTX)),
    ];
    println!("\n=== builder-derived ids and prices ===");
    for (name, prof, (pf, de)) in rows.iter() {
        println!(
            "{name}\n  shape_profile_id   {}\n  canonical_class_id {}\n  canonical job      ({pf}, {de})\n  draw MAC-eq        {}",
            prof.shape_profile_id(),
            canon_id(prof),
            draw_ccu(prof, *pf, *de)
        );
    }

    // Which lookup table answers each genesis class id?
    let typed = kaspa_consensus_core::palw_model_registry_v1::palw_rc_typed_class_works_v1();
    let genesis = kaspa_consensus_core::palw_model_registry_v1::palw_genesis_model_works_v1(&bundle.genesis_objects);
    println!("\n=== which table describes each t12 genesis class ===");
    for o in bundle.genesis_objects.iter() {
        if let PalwConsensusObjectV2::ClassRegistered { class_id, .. } = o {
            println!(
                "{class_id}  bundle_carriage={}  rc_typed={}  (neither => falls through to the build's canonical_classes_v1 table)",
                genesis.contains_key(class_id),
                typed.contains_key(class_id)
            );
        }
    }
    println!("\nrc_typed table keys ({}):", typed.len());
    for k in typed.keys() {
        println!("  {k}");
    }
}

/// The canonical job is NOT an input to the class id — the same graph prices at any (P, D).
#[test]
fn b2_01_the_canonical_job_is_not_in_the_class_id() {
    for (name, prof, n_ctx) in [("floor", floor(), 12u32), ("dense@2M", dense(), PALW_T12_DENSE_N_CTX)] {
        let id = prof.shape_profile_id();
        let cid = canon_id(&prof);
        let small = draw_ccu(&prof, 1, 2);
        let big = draw_ccu(&prof, n_ctx.saturating_sub(1).max(1), 2);
        println!(
            "{name}: one class id {id} / canonical {cid}\n  price at (1,2)          = {small} MAC-eq\n  price at ({},2) = {big} MAC-eq   ratio {:.1}x",
            n_ctx - 1,
            big as f64 / small.max(1) as f64
        );
        assert_eq!(id, prof.shape_profile_id(), "the id is a function of the profile alone");
    }
}

/// Field sweep. For every mutation: does shape_profile_id move? canonical_class_id_v1? the price?
#[test]
fn b2_02_field_sweep_committed_versus_priced() {
    let base = dense();
    let (pf, de) = qwen25_a16_held_canonical_v1(PALW_T12_DENSE_N_CTX);
    // A short job so the sweep is cheap; the price is a closed form so the length is irrelevant
    // to whether a field is READ.
    let (pf, de) = (pf.min(64), de);
    let base_shape = base.shape_profile_id();
    let base_canon = canon_id(&base);
    let base_price = draw_ccu(&base, pf, de);

    let mut muts: Vec<(&str, PalwShapeProfileV3)> = Vec::new();
    let mut push = |name: &'static str, f: &dyn Fn(&mut PalwShapeProfileV3)| {
        let mut p = base.clone();
        f(&mut p);
        muts.push((name, p));
    };
    push("version 1->2 (layer-kind phase)", &|p| p.version = 2);
    push("lane Float32<->Int32", &|p| {
        p.lane = match p.lane {
            kaspa_consensus_core::palw_step::PalwStepLaneV1::Int32 => kaspa_consensus_core::palw_step::PalwStepLaneV1::Float32,
            kaspa_consensus_core::palw_step::PalwStepLaneV1::Float32 => kaspa_consensus_core::palw_step::PalwStepLaneV1::Int32,
        }
    });
    push("layer_count +1", &|p| p.layer_count += 1);
    push("hidden_dim +1", &|p| p.hidden_dim += 1);
    push("ffn_dim +1", &|p| p.ffn_dim += 1);
    push("n_ctx /2", &|p| p.n_ctx /= 2);
    push("attn_heads +1", &|p| p.attn_heads += 1);
    push("attn_kv_heads +1", &|p| p.attn_kv_heads += 1);
    push("attn_head_dim +1", &|p| p.attn_head_dim += 1);
    push("rope_dims +1", &|p| p.rope_dims += 1);
    push("rope_freq_base_bits ^1", &|p| p.rope_freq_base_bits ^= 1);
    push("vocab_size +1", &|p| p.vocab_size += 1);
    push("kv_cache_f16 ^1", &|p| p.kv_cache_f16 ^= 1);
    push("gdn_heads +1", &|p| p.gdn_heads += 1);
    push("full_attention_interval +1", &|p| p.full_attention_interval += 1);
    push("logits_scheme_id -> other", &|p| p.logits_scheme_id = Hash64::from_u64_word(0xDEAD));
    push("state_chunk_map_id -> other", &|p| p.state_chunk_map_id = Hash64::from_u64_word(0xBEEF));
    push("kv_chunk_calls +1", &|p| p.kv_chunk_calls += 1);
    push("n_batch /2", &|p| p.n_batch = (p.n_batch / 2).max(1));
    push("n_ubatch /2", &|p| p.n_ubatch = (p.n_ubatch / 2).max(1));
    push("n_seq +1", &|p| p.n_seq += 1);
    push("n_threads +1", &|p| p.n_threads += 1);
    push("repack_on ^1", &|p| p.repack_on ^= 1);
    push("llamafile_on ^1", &|p| p.llamafile_on ^= 1);
    push("fused_gdn_on ^1", &|p| p.fused_gdn_on ^= 1);
    push("use_ref_off ^1", &|p| p.use_ref_off ^= 1);
    push("base0_rms_eps_q +1", &|p| p.base0_rms_eps_q += 1);
    push("every node tile_len -> 64", &|p| {
        for t in [&mut p.pre_nodes, &mut p.gdn_nodes, &mut p.attn_nodes, &mut p.post_nodes] {
            for n in t.iter_mut() {
                n.tile_len = 64;
            }
        }
    });
    push("post_nodes[0].weight_dtypes -> F32(0? no: 26/I32)", &|p| {
        if let Some(n) = p.post_nodes.iter_mut().find(|n| !n.weight_dtypes.is_empty()) {
            for d in n.weight_dtypes.iter_mut() {
                *d = 26; // I32 -> dtype cost 4
            }
        }
    });
    push("pre_nodes[0].weight_name += \".routed\"", &|p| {
        if let Some(n) = p.pre_nodes.first_mut() {
            n.weight_name.push_str(".routed");
        }
    });
    push("reference_ruleset_id -> other", &|p| p.reference_ruleset_id = Hash64::from_u64_word(0xF00D));

    println!("\n{:<46} {:>9} {:>9} {:>9}", "mutation", "shape_id", "canon_id", "price");
    println!("{}", "-".repeat(78));
    let mut uncommitted_but_priced: Vec<&str> = Vec::new();
    for (name, p) in muts.iter() {
        let s_moved = p.shape_profile_id() != base_shape;
        let c = PalwCanonicalClassDescriptorV1::of(p, Hash64::default()).map(|d| d.canonical_class_id_v1());
        let c_moved = match c {
            Ok(h) => h != base_canon,
            Err(_) => true, // mixed weight format: the descriptor refuses, which is a move
        };
        let price = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let job = rc_job_context(p, pf, de);
            palw_attempt_economic_compute_v1(p, &job, true, &PALW_ECONOMIC_COST_TABLE_V1).ok()
        }))
        .ok()
        .flatten();
        let p_moved = match price {
            Some(v) => v != base_price,
            None => false, // refused; not a price move
        };
        println!(
            "{name:<46} {:>9} {:>9} {:>9}",
            if s_moved { "MOVED" } else { "same" },
            if c_moved { "MOVED" } else { "SAME" },
            if p_moved { "MOVED" } else { "same" }
        );
        if p_moved && !c_moved {
            uncommitted_but_priced.push(name);
        }
    }
    println!("\nbase price at ({pf},{de}) = {base_price} MAC-eq");
    println!("\nPRICED but NOT in canonical_class_id_v1's preimage: {uncommitted_but_priced:?}");
}

/// **The canonical job enters the genesis commitment ONLY through its step-leaf count.**
///
/// `palw_genesis_v2.rs:229` checks `pwu_per_inference == entry.canonical_step_leaf_count` and
/// nothing else about `(P, D)`. Past `palw_canonical_work` (armed at DAA 0 on t12) the quantity
/// that actually prices every attempt is the DERIVED MAC-eq of one draw, a different function of
/// the same `(P, D)`. So a pair that collides on the leaf count mints a byte-identical genesis and
/// prices every claim differently.
#[test]
fn b2_03_leaf_count_collision_reprices_the_class() {
    use kaspa_consensus_core::palw_step::step_leaf_count;

    for (name, prof, n_ctx) in
        [("BASE-0 floor", floor(), 12u32), ("Qwen3.6 v7@512", hybrid(), PALW_T12_HYBRID_N_CTX)]
    {
        let mut by_leaves: std::collections::BTreeMap<u64, Vec<((u32, u32), u128)>> = Default::default();
        for pf in 1..=n_ctx {
            for de in 1..=n_ctx {
                if (pf as u64) + (de.max(1) as u64) - 1 > n_ctx as u64 {
                    continue;
                }
                let job = rc_job_context(&prof, pf, de);
                let Ok(leaves) = step_leaf_count(&prof, &job) else { continue };
                let price = draw_ccu(&prof, pf, de);
                by_leaves.entry(leaves).or_default().push(((pf, de), price));
            }
        }
        let mut worst: Option<(u64, (u32, u32), u128, (u32, u32), u128)> = None;
        for (leaves, v) in by_leaves.iter() {
            let lo = v.iter().min_by_key(|(_, p)| *p).unwrap();
            let hi = v.iter().max_by_key(|(_, p)| *p).unwrap();
            if lo.1 != hi.1 {
                let better = worst.map_or(true, |(_, _, wl, _, wh)| (hi.1 as f64 / lo.1 as f64) > (wh as f64 / wl as f64));
                if better {
                    worst = Some((*leaves, lo.0, lo.1, hi.0, hi.1));
                }
            }
        }
        match worst {
            Some((leaves, lo_job, lo_price, hi_job, hi_price)) => println!(
                "{name}: COLLISION at canonical_step_leaf_count = {leaves}\n  \
                 (P,D) {lo_job:?} -> draw {lo_price} MAC-eq\n  \
                 (P,D) {hi_job:?} -> draw {hi_price} MAC-eq   ratio {:.4}x\n  \
                 both mint pwu_per_inference = {leaves}, the same catalog entry, the same class id.",
                hi_price as f64 / lo_price as f64
            ),
            None => println!("{name}: no exact leaf-count collision over P+D-1 <= {n_ctx}"),
        }
    }
}

/// The floor's priced unit is a COMPILE-TIME constant looked up by class id, not chain data.
#[test]
fn b2_04_the_floor_price_is_a_build_constant() {
    let p = t12_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!() };
    let f = floor();
    let id = f.shape_profile_id();
    assert_eq!(id, bundle.base_class_id, "the t12 floor IS this build's base0 profile");

    let typed = kaspa_consensus_core::palw_model_registry_v1::palw_rc_typed_class_works_v1();
    let work = typed.get(&id).expect("the floor resolves through the build's typed table");
    println!(
        "t12 floor class {id}\n  chain-committed pwu_per_inference (declared leaves) = 7708\n  \
         build-derived economic_ccu_per_claim (MAC-eq/draw)  = {}\n  \
         verification_ccu                                    = {}\n  \
         the (P,D) behind both is PALW_RC_BASE0_CANONICAL = {:?}, a `pub const` in \
         consensus/core/src/palw_base0_profile.rs:905 — it appears in NO chain object.",
        work.economic_ccu_per_claim, work.verification_ccu, PALW_RC_BASE0_CANONICAL
    );

    // What the same class prices at if the build shipped a different canonical job.
    for cand in [(8u32, 4u32), (8, 2), (11, 2), (1, 2), (4, 4)] {
        println!("  build const {cand:?} -> draw {} MAC-eq", draw_ccu(&f, cand.0, cand.1));
    }
}

/// **The MoE hypothesis: what decides "this matmul is block-diagonal over k experts"?**
///
/// `palw_economic_compute_v1::routed_group_count` (`:296-306`) finds the first node in the SAME
/// table with `op_kind == SoftMax` and `weight_name.contains("router")`, and reads
/// `k = out_len.elements / 2`. `node_cost` (`:365-378`) then divides the routed matmul's MACs by
/// `k`. Both `weight_name` and `out_len` are registrant-written fields of the profile.
#[test]
fn b2_05_the_active_expert_count_is_a_declared_out_width_and_a_string() {
    use kaspa_consensus_core::palw_step::{PalwStepNodeV1, PalwStepOpKindV1, PalwStepOutLenV1};

    let base = hybrid();
    let (pf, de) = qwen36_held_canonical_v1(PALW_T12_HYBRID_N_CTX);
    let base_price = draw_ccu(&base, pf, de);

    // Report every node the routing rule looks at.
    for (tname, t) in [("pre", &base.pre_nodes), ("gdn", &base.gdn_nodes), ("attn", &base.attn_nodes), ("post", &base.post_nodes)] {
        for (i, n) in t.iter().enumerate() {
            let is_router = n.op_kind == PalwStepOpKindV1::SoftMax && n.weight_name.contains("router");
            let is_routed = n.weight_name.ends_with(".routed");
            if is_router || is_routed {
                println!(
                    "{tname}[{i}] op {:?} name {:?} out {:?} {}{}",
                    n.op_kind,
                    n.weight_name,
                    n.out_len,
                    if is_router { "<= ROUTER (k = elements/2)" } else { "" },
                    if is_routed { "<= .routed (cost / k)" } else { "" }
                );
            }
        }
    }

    let retune = |f: &dyn Fn(&mut PalwStepNodeV1)| -> (u128, Hash64) {
        let mut p = base.clone();
        for t in [&mut p.gdn_nodes, &mut p.attn_nodes] {
            for n in t.iter_mut() {
                if n.op_kind == PalwStepOpKindV1::SoftMax && n.weight_name.contains("router") {
                    f(n);
                }
            }
        }
        let price = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| draw_ccu(&p, pf, de))).unwrap_or(0);
        (price, p.shape_profile_id())
    };

    let (k1_by_width, id_w) = retune(&|n| n.out_len = PalwStepOutLenV1::Fixed { elements: 2 });
    let (k1_by_name, id_n) = retune(&|n| n.weight_name = n.weight_name.replace("router", "rtr"));
    let (k256, id_k) = retune(&|n| n.out_len = PalwStepOutLenV1::Fixed { elements: 512 });

    println!(
        "\nt12 hybrid Qwen3.6 v7@512 canonical ({pf},{de}), draw price in MAC-eq:\n  \
         as registered (router out 16 => k = 8)                 {base_price}\n  \
         router out_len 16 -> 2   (k = 1, 'dense')              {k1_by_width}   x{:.4}\n  \
         router renamed  'router' -> 'rtr' (rule finds none)    {k1_by_name}   x{:.4}\n  \
         router out_len 16 -> 512 (k = 256, all experts)        {k256}   x{:.4}\n  \
         class ids: base {}\n             out2 {id_w}\n             rtr  {id_n}\n             k256 {id_k}",
        k1_by_width as f64 / base_price as f64,
        k1_by_name as f64 / base_price as f64,
        k256 as f64 / base_price as f64,
        base.shape_profile_id(),
    );
}

/// **`PalwCanonicalClassDescriptorV1::canonical_class_id_v1` does not hash `profile.version`,
/// and `version` selects the layer-kind phase the cost model sums over.**
///
/// `palw_canonical_work_v1.rs:242-296` hashes `lane` but never `p.version`;
/// `PalwShapeProfileV3::layer_kind` (`palw_step.rs:753-762`) reads it, and
/// `palw_economic_shape_v1` (`palw_economic_compute_v1.rs:439-463`) sums the attention table
/// over the Attention layers and the GDN table over the rest. With `layer_count % interval != 0`
/// the two phases give a different SPLIT, hence a different price. The split is normally re-pinned
/// by `weight_dtypes.len() == table_layer_span`, which IS hashed — so this pair uses weightless
/// layer tables, where nothing re-pins it.
#[test]
fn b2_06_same_canonical_class_id_different_compute() {
    use kaspa_consensus_core::palw_step::{PalwStepLaneV1, PalwStepNodeRoleV1, PalwStepNodeV1, PalwStepOpKindV1, PalwStepOutLenV1};

    let node = |op: PalwStepOpKindV1, elements: u32, refs: Vec<u16>| PalwStepNodeV1 {
        op_kind: op,
        role: PalwStepNodeRoleV1::Plain,
        weight_name: String::new(),
        weight_dtypes: Vec::new(),
        out_len: PalwStepOutLenV1::Fixed { elements },
        tile_len: 64,
        kernel_semantics_id: Hash64::from_u64_word(op as u64 + 1),
        input_refs: refs,
    };

    let build = |version: u16| PalwShapeProfileV3 {
        version,
        lane: PalwStepLaneV1::Int32,
        layer_count: 41,
        full_attention_interval: 4,
        hidden_dim: 256,
        ffn_dim: 512,
        attn_heads: 4,
        attn_kv_heads: 2,
        attn_head_dim: 64,
        rope_dims: 32,
        rope_sections: [0; 4],
        rope_freq_base_bits: 0x4B18_9680,
        rms_eps_bits: 0,
        l2_eps_bits: 0,
        base0_rms_eps_q: 1,
        logits_scheme_id: Hash64::from_u64_word(0x5C_1E),
        gdn_heads: 4,
        gdn_head_k_dim: 16,
        gdn_head_v_dim: 16,
        gdn_conv_kernel: 4,
        vocab_size: 1024,
        repack_on: 0,
        llamafile_on: 0,
        flash_attn_disabled: 1,
        fused_gdn_on: 0,
        use_ref_off: 1,
        kv_cache_f16: 0,
        n_ctx: 12,
        n_batch: 1,
        n_ubatch: 1,
        n_seq: 1,
        n_threads: 1,
        // The attention table is priced per element; the GDN table is priced per state element.
        pre_nodes: vec![node(PalwStepOpKindV1::EmbedLookup, 256, vec![0xFFFF])],
        gdn_nodes: vec![node(PalwStepOpKindV1::GatedDeltaNet, 256, vec![0xFFFF])],
        attn_nodes: vec![node(PalwStepOpKindV1::RmsNorm, 256, vec![0xFFFF])],
        post_nodes: vec![node(PalwStepOpKindV1::EmbedLookup, 1024, vec![0xFFFF])],
        reference_ruleset_id: Hash64::from_u64_word(0x2_1E5),
        transcendental_bindings: Vec::new(),
        contraction_facts: Vec::new(),
        kv_chunk_calls: 0,
        state_chunk_map_id: Hash64::default(),
    };

    let v1 = build(1);
    let v2 = build(2);
    v1.validate_shape().expect("v1 is a legal shape");
    v2.validate_shape().expect("v2 is a legal shape");

    let attn_v1 = (0..v1.layer_count).filter(|l| v1.layer_kind(*l) == kaspa_consensus_core::palw_step::PalwLayerKindV1::Attention).count();
    let attn_v2 = (0..v2.layer_count).filter(|l| v2.layer_kind(*l) == kaspa_consensus_core::palw_step::PalwLayerKindV1::Attention).count();

    let p1 = draw_ccu(&v1, 11, 2);
    let p2 = draw_ccu(&v2, 11, 2);
    println!(
        "layer_count 41, interval 4, weightless layer tables\n  \
         version 1: {attn_v1} attention / {} gdn -> draw {p1} MAC-eq\n  \
         version 2: {attn_v2} attention / {} gdn -> draw {p2} MAC-eq   ratio {:.6}x\n  \
         shape_profile_id      v1 {}\n                        v2 {}\n  \
         canonical_class_id_v1 v1 {}\n                        v2 {}",
        v1.layer_count as usize - attn_v1,
        v2.layer_count as usize - attn_v2,
        p2 as f64 / p1 as f64,
        v1.shape_profile_id(),
        v2.shape_profile_id(),
        canon_id(&v1),
        canon_id(&v2),
    );
    assert_eq!(canon_id(&v1), canon_id(&v2), "the canonical class identity does not separate them");
    assert_ne!(p1, p2, "and the cost model prices them differently");
}

/// **The declared per-tensor quantization is a pure price field.**
///
/// `palw_weight_dtype_cost_v1` (`palw_economic_compute_v1.rs:93-106`) charges F32 = 4, F16/BF16/I16
/// = 2 and everything else = 1 per MAC; `dtype_cost` (`:330`) reads `node.weight_dtypes[layer]`.
/// Repo-wide, the only other readers of `weight_dtypes` are `palw_class_weight_dtype_v1` (whose
/// caller `PalwCanonicalClassDescriptorV1::of` has no production call site) and one plan gate in
/// `misaka-palw-base0/src/qwen36_plan.rs:1133`. The A16 (dense) backend contains no dtype check at
/// all, the adjudicator (`palw_step_refute`) never reads the field, and the inventory root — the
/// one value that binds the artifact — is built from tile geometry, not dtypes.
#[test]
fn b2_07_declared_dtype_prices_the_class_and_binds_to_no_artifact() {
    let base = dense();
    let (pf, de) = qwen25_a16_held_canonical_v1(PALW_T12_DENSE_N_CTX);
    let base_price = draw_ccu(&base, pf, de);

    let declared: std::collections::BTreeSet<u8> = [&base.pre_nodes, &base.gdn_nodes, &base.attn_nodes, &base.post_nodes]
        .iter()
        .flat_map(|t| t.iter())
        .flat_map(|n| n.weight_dtypes.iter().copied())
        .collect();
    println!("t12 dense row declares weight dtypes {declared:?} (cost {:?} per MAC)",
        declared.iter().map(|d| kaspa_consensus_core::palw_economic_compute_v1::palw_weight_dtype_cost_v1(*d)).collect::<Vec<_>>());

    for (name, code) in [("I8/Q* (as registered)", 24u8), ("F16", 1u8), ("I32", 26u8), ("F32", 0u8)] {
        let mut p = base.clone();
        for t in [&mut p.pre_nodes, &mut p.gdn_nodes, &mut p.attn_nodes, &mut p.post_nodes] {
            for n in t.iter_mut() {
                for d in n.weight_dtypes.iter_mut() {
                    *d = code;
                }
            }
        }
        if p.validate_shape().is_err() {
            println!("  {name:<22} code {code:>3}: refused by validate_shape");
            continue;
        }
        let price = draw_ccu(&p, pf, de);
        println!(
            "  {name:<22} code {code:>3}: draw {price} MAC-eq  x{:.4}   class id {}",
            price as f64 / base_price as f64,
            p.shape_profile_id()
        );
    }
}

/// The dtype lever across all three t12 rows and across context depths.
#[test]
fn b2_08_dtype_lever_across_the_card() {
    let rows: [(&str, PalwShapeProfileV3, (u32, u32)); 3] = [
        ("BASE-0 floor", floor(), PALW_RC_BASE0_CANONICAL),
        ("Qwen3.6 v7@512", hybrid(), qwen36_held_canonical_v1(PALW_T12_HYBRID_N_CTX)),
        ("Qwen2.5 A16 v7@2M", dense(), qwen25_a16_held_canonical_v1(PALW_T12_DENSE_N_CTX)),
    ];
    for (name, base, (pf, de)) in rows.iter() {
        let b = draw_ccu(base, *pf, *de);
        let mut p = base.clone();
        for t in [&mut p.pre_nodes, &mut p.gdn_nodes, &mut p.attn_nodes, &mut p.post_nodes] {
            for n in t.iter_mut() {
                for d in n.weight_dtypes.iter_mut() {
                    *d = 26; // I32: palw_weight_dtype_cost_v1 = 4
                }
            }
        }
        if p.validate_shape().is_err() {
            println!("{name}: I32 declaration refused by validate_shape");
            continue;
        }
        let w = draw_ccu(&p, *pf, *de);
        // and at a shallow job, where the matmul terms are not swamped by the attention sum
        let (sb, sw) = (draw_ccu(base, 1, 2), draw_ccu(&p, 1, 2));
        println!(
            "{name} canonical ({pf},{de}): I8 {b} -> I32 {w}  x{:.4}   |  at (1,2): {sb} -> {sw}  x{:.4}",
            w as f64 / b as f64,
            sw as f64 / sb as f64
        );
    }
}

/// Where the dtype lever lands as fork-choice weight: above the ADR-0137 work floor W0 the class
/// target saturates at u128::MAX, expected_attempts = 1, and `claim.pwu == economic_ccu_per_claim`
/// — so the declared dtype multiplies the weight 1:1.
#[test]
fn b2_09_the_dtype_lever_as_fork_weight() {
    use kaspa_consensus_core::palw_pwu::{palw_expected_attempts_v1, palw_pwu_v1};
    use kaspa_consensus_core::palw_work_target_v1::{palw_work_floor_v1, palw_work_ticket_target_v1};

    // t12: escrow = calc_block_subsidy(0) * worker_carve 720/1000 ; rate = 900_000_000 sompi/Giga.
    const ESCROW_SOMPI: u64 = 320_084_650_080;
    const RATE: u64 = 900_000_000;
    let w0 = palw_work_floor_v1(ESCROW_SOMPI, RATE);
    println!("t12 work floor W0 = {w0} MAC-eq (escrow {ESCROW_SOMPI} sompi / rate {RATE} per 1e9)");

    let base = dense();
    let mut i32_row = base.clone();
    for t in [&mut i32_row.pre_nodes, &mut i32_row.gdn_nodes, &mut i32_row.attn_nodes, &mut i32_row.post_nodes] {
        for n in t.iter_mut() {
            for d in n.weight_dtypes.iter_mut() {
                *d = 26;
            }
        }
    }
    println!("\n{:>10} {:>22} {:>22} {:>8} {:>22} {:>22} {:>8}", "prefill", "honest CCU", "lied CCU", "price x", "honest pwu", "lied pwu", "weight x");
    for prefill in [64u32, 256, 1024, 4096, 16384, 65536, 262143] {
        let h = draw_ccu(&base, prefill, 2);
        let l = draw_ccu(&i32_row, prefill, 2);
        let pwu = |ccu: u128| {
            let t = palw_work_ticket_target_v1(ccu, w0);
            let a = palw_expected_attempts_v1(t);
            (palw_pwu_v1(t, ccu.min(u64::MAX as u128) as u64), a)
        };
        let (hp, ha) = pwu(h);
        let (lp, la) = pwu(l);
        println!(
            "{prefill:>10} {h:>22} {l:>22} {:>7.4}x {hp:>22} {lp:>22} {:>7.4}x   (attempts {ha} vs {la})",
            l as f64 / h as f64,
            lp as f64 / hp as f64
        );
    }
}

/// Does the t12 admission gate actually admit a dense row that declares I32 weights?
#[test]
fn b2_10_the_admission_gate_on_an_i32_declaration() {
    use kaspa_consensus_core::palw_class_admission_v2::{palw_admission_shape_at_v1, verify_class_admission_v9};
    use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwClassAdmissionCarriageV2, PalwPwuRuleV2};

    let params = t12_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!() };
    let certified = kaspa_consensus_core::palw_e2e_adjudicability::palw_rc_certified_families_v1();
    let chain_certified: Vec<kaspa_consensus_core::palw_e2e_adjudicability::PalwE2eFamilyV1> = Vec::new();

    for (name, code) in [("as registered (I8 = 24)", 24u8), ("declared I32 (= 26)", 26u8), ("declared F16 (= 1)", 1u8)] {
        for n_ctx in [512u32, PALW_T12_DENSE_N_CTX] {
            let mut p = qwen25_a16_artifact_row_profile_v7(PalwQwen25GeometryV1 { n_ctx, ..QWEN25_1_5B }).expect("row");
            for t in [&mut p.pre_nodes, &mut p.gdn_nodes, &mut p.attn_nodes, &mut p.post_nodes] {
                for node in t.iter_mut() {
                    for d in node.weight_dtypes.iter_mut() {
                        *d = code;
                    }
                }
            }
            let (pf, de) = if n_ctx == 512 { (256u32, 2u32) } else { qwen25_a16_held_canonical_v1(n_ctx) };
            let canonical = rc_job_context(&p, pf, de);
            let shape = match palw_admission_shape_at_v1(&params, bundle, &p, 0) {
                Ok(s) => s,
                Err(e) => {
                    println!("{name} @n_ctx {n_ctx}: no admission shape: {e}");
                    continue;
                }
            };
            let ladder_cap = shape.ladder.map(|r| r.ladder).unwrap_or_else(|| bundle.court.max_step_leaf_count());
            let counted = match kaspa_consensus_core::palw_step::step_leaf_count_capped_v1(&p, &canonical, ladder_cap) {
                Ok(c) => c,
                Err(e) => {
                    println!("{name} @n_ctx {n_ctx} ({pf},{de}): leaf count refused: {e:?}");
                    continue;
                }
            };
            let object = PalwConsensusObjectV2::ClassRegistered {
                class_id: p.shape_profile_id(),
                artifact_root: Hash64::from_u64_word(0xA5757),
                slash_value_per_pwu: 5,
                pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: counted },
                initial_target: u128::MAX,
                share_permille: 1,
                activation_daa: 0,
                admission: Some(Box::new(PalwClassAdmissionCarriageV2 {
                    profile: p.clone(),
                    canonical: canonical.clone(),
                    registrant_bond: PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(
                        kaspa_consensus_core::tx::TransactionId::default(),
                        0,
                    )),
                    signature: Vec::new(),
                })),
            };
            let verdict = verify_class_admission_v9(
                bundle,
                &p,
                &canonical,
                &object,
                &certified,
                &chain_certified,
                shape.ladder,
                shape.court,
                false,
                shape.token_lift,
                shape.fused_dissectable,
                true, // palw_canonical_work armed at 0 on t12
                shape.held,
                shape.kimi_family,
                params.palw_audit_2026_09_23_active_at(0),
            );
            println!(
                "{name} @n_ctx {n_ctx} canonical ({pf},{de}) leaves {counted}: {}   draw {} MAC-eq",
                match &verdict {
                    Ok(_) => "ADMITTED".to_string(),
                    Err(e) => format!("refused: {e}"),
                },
                draw_ccu(&p, pf, de)
            );
        }
    }
}
