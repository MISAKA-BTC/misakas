//! **RFC-0002 Phase F, step F8: an IR class's work vector** — from the structure alone.
//!
//! * The vector reads nothing a registrant lays out: re-committing a program (every committable node
//!   marked a commit point, or every optional commit point unmarked) leaves it unchanged, and no
//!   layout is an input at all (PALW-TIR-16, PALW-WK-3).
//! * The closed form is the walk: position by position, occurrence by occurrence, node by node at
//!   the position's `H`, over a sweep of jobs (prompt, generated tokens, reused prefix).
//! * The corpus: each architecture's work lands in the dimension its structure names — weights in
//!   `dense_matmul`, the router's experts in `routed_expert_matmul`, a recurrence in `recurrence`, a
//!   history in the attention and KV dimensions, norms in `normalization`.

use std::path::PathBuf;

use kaspa_consensus_core::palw_canonical_work_v1::{PalwCanonicalExecutionFactsV1, PalwCanonicalWorkVectorV1};
use kaspa_consensus_core::palw_tir_work_v1::{
    PalwTirWorkKindV1, palw_tir_canonical_work_v1, palw_tir_model_work_v1, palw_tir_node_work_v1, palw_tir_work_kinds_v1,
    palw_tir_work_shape_v1,
};
use misaka_palw_tir::{Prim, TirProgramV1};

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex")).collect()
}

fn programs() -> Vec<(String, TirProgramV1)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../consensus-vectors/tir-v1/programs");
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("vectors")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    files
        .into_iter()
        .map(|path| {
            let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            let p = TirProgramV1::decode_canonical(&unhex(v["program_borsh_hex"].as_str().unwrap())).expect("canonical");
            (v["name"].as_str().unwrap().to_string(), p)
        })
        .collect()
}

fn job(prefill: u32, generated: u32, reused: u32) -> PalwCanonicalExecutionFactsV1 {
    PalwCanonicalExecutionFactsV1 { reused_prefix_tokens: reused, ..PalwCanonicalExecutionFactsV1::uncached(prefill, generated) }
}

/// A commit point the normal form requires: a carry-out, the logits, a `TopK`, an appended row.
fn required_commits(p: &TirProgramV1, block: usize) -> Vec<bool> {
    let b = &p.blocks[block];
    let mut req = vec![false; b.nodes.len()];
    for n in &b.carry_out {
        req[*n as usize] = true;
    }
    if block == p.schedule.post as usize {
        req[p.logits as usize] = true;
    }
    for (i, n) in b.nodes.iter().enumerate() {
        if matches!(n.prim, Prim::TopK { .. }) {
            req[i] = true;
        }
        if let Prim::HistAppend { .. } = n.prim
            && let misaka_palw_tir::Ref::Node(j) = n.inputs[0]
        {
            req[j as usize] = true;
        }
    }
    req
}

#[test]
fn the_vector_reads_nothing_a_registrant_lays_out() {
    let facts = job(5, 4, 0);
    for (name, p) in programs() {
        let base = palw_tir_canonical_work_v1(&p, &facts).unwrap_or_else(|e| panic!("{name}: {e}"));
        // Every committable node committed.
        let mut all = p.clone();
        for b in all.blocks.iter_mut() {
            for n in b.nodes.iter_mut() {
                if n.out.dtype.committable() {
                    n.commit = true;
                }
            }
        }
        misaka_palw_tir::validate::validate(&all).unwrap_or_else(|e| panic!("{name}: still a program: {e}"));
        assert_eq!(palw_tir_canonical_work_v1(&all, &facts).unwrap(), base, "{name}: committing more moves nothing");
        // Only the commit points normal form requires.
        let mut few = p.clone();
        for bi in 0..few.blocks.len() {
            let req = required_commits(&few, bi);
            for (i, n) in few.blocks[bi].nodes.iter_mut().enumerate() {
                n.commit = req[i];
            }
        }
        misaka_palw_tir::validate::validate(&few).unwrap_or_else(|e| panic!("{name}: still a program: {e}"));
        assert_eq!(palw_tir_canonical_work_v1(&few, &facts).unwrap(), base, "{name}: committing less moves nothing");
    }
}

/// The walk: every executed position, every occurrence it runs, every node at its `H`.
fn walk(p: &TirProgramV1, facts: &PalwCanonicalExecutionFactsV1) -> PalwCanonicalWorkVectorV1 {
    let info = misaka_palw_tir::validate::validate(p).unwrap();
    let occurrences = p.occurrences();
    let post = occurrences.len() - 1;
    let (prefill, end) = (facts.prefill_tokens, facts.prefill_tokens + facts.generated_tokens - 1);
    let mut v = PalwCanonicalWorkVectorV1::default();
    for a in facts.reused_prefix_tokens.min(prefill - 1)..end {
        for (o, (block, _)) in occurrences.iter().enumerate() {
            let runs = if o == post { a + 1 >= prefill } else { a >= facts.reused_prefix_tokens };
            if !runs {
                continue;
            }
            let bi = *block as usize;
            let h = info.blocks[bi].window.map(|w| (a as u64 + 1).min(w as u64)).unwrap_or(1);
            let kinds = palw_tir_work_kinds_v1(p, bi);
            for (ni, kind) in kinds.iter().enumerate() {
                let [mac, weight, kv_read, kv_write] = palw_tir_node_work_v1(p, bi, ni, *kind, h);
                match kind {
                    PalwTirWorkKindV1::DenseMatmul => v.dense_matmul += mac,
                    PalwTirWorkKindV1::RoutedExpertMatmul => v.routed_expert_matmul += mac,
                    PalwTirWorkKindV1::Normalization => v.normalization += mac,
                    PalwTirWorkKindV1::Recurrence => v.recurrence += mac,
                    PalwTirWorkKindV1::Attention if a < prefill => v.attention_prefill += mac,
                    PalwTirWorkKindV1::Attention => v.attention_decode += mac,
                    PalwTirWorkKindV1::Other => v.other_verified_ops += mac,
                }
                v.weight_traffic_bytes += weight;
                v.kv_read_bytes += kv_read;
                v.kv_write_bytes += kv_write;
            }
        }
    }
    v
}

#[test]
fn the_closed_form_is_the_walk() {
    let mut checked = 0;
    for (name, p) in programs() {
        let shape = palw_tir_work_shape_v1(&p).unwrap_or_else(|e| panic!("{name}: {e}"));
        for (prefill, generated, reused) in [(1, 1, 0), (1, 5, 0), (3, 1, 0), (4, 4, 0), (7, 9, 2), (9, 2, 9), (12, 12, 5), (2, 20, 1)]
        {
            let facts = job(prefill, generated, reused);
            assert_eq!(shape.work_v1(&facts).unwrap(), walk(&p, &facts), "{name} P {prefill} G {generated} reused {reused}");
            checked += 1;
        }
    }
    assert_eq!(checked, 7 * 8);
}

#[test]
fn each_architecture_lands_in_the_dimension_its_structure_names() {
    let facts = job(6, 4, 0);
    for (name, p) in programs() {
        let v = palw_tir_canonical_work_v1(&p, &facts).unwrap();
        eprintln!("{name:>24}: {v:?}");
        let has = |pred: &dyn Fn(&Prim) -> bool| p.blocks.iter().any(|b| b.nodes.iter().any(|n| pred(&n.prim)));
        let hist = has(&|x| matches!(x, Prim::HistAppend { .. }));
        let state = has(&|x| matches!(x, Prim::StateWrite { .. }));
        let topk = has(&|x| matches!(x, Prim::TopK { .. }));
        let rsqrt = has(&|x| matches!(x, Prim::IntRsqrt));
        assert_eq!(v.dense_matmul > 0, !p.params.is_empty(), "{name}: weights stream iff the program has params");
        assert_eq!(v.routed_expert_matmul > 0, topk, "{name}: routed experts iff a router selects them");
        assert_eq!(v.recurrence > 0, state, "{name}: a recurrence iff a Fixed state is written");
        assert_eq!(v.kv_read_bytes > 0 && v.kv_write_bytes > 0, hist, "{name}: KV traffic iff a history is kept");
        assert_eq!(v.attention_prefill > 0 && v.attention_decode > 0, hist, "{name}: attention iff a history is read");
        assert_eq!(v.normalization > 0, rsqrt, "{name}: normalization iff a norm's IntRsqrt exists");
        assert!(v.other_verified_ops > 0, "{name}");
        if v.dense_matmul > 0 {
            assert!(v.weight_traffic_bytes > 0, "{name}: a weight matmul streams its weights");
        }
    }
}

#[test]
fn the_registry_work_is_the_same_walk() {
    for (name, p) in programs() {
        let z = kaspa_consensus_core::Hash64::from_bytes([0; 64]);
        let canonical = kaspa_consensus_core::palw_v2::PalwJobContextV2 {
            version: 2,
            network_id: b"testnet-12".to_vec(),
            job_id: z,
            job_nullifier: z,
            assignment_id: z,
            execution_seed: [0; 32],
            model_profile_id: z,
            runtime_manifest_hash: z,
            runtime_class_id: z,
            shape_profile_id: z,
            trace_scheme_id: z,
            cu_ruleset_id: z,
            tokenizer_id: z,
            prompt_token_ids_hash: z,
            declared_prefill_tokens: 6,
            exact_decode_tokens: 4,
            max_context_tokens: 64,
        };
        let work = palw_tir_model_work_v1(&p, &canonical).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(work.verification_ccu, palw_tir_canonical_work_v1(&p, &job(6, 4, 0)).unwrap().provisional_scalar_v1());
        assert_eq!(work.economic_ccu_per_claim, palw_tir_canonical_work_v1(&p, &job(6, 1, 0)).unwrap().provisional_scalar_v1());
        assert!(work.verification_ccu >= work.economic_ccu_per_claim, "{name}: the canonical job is at least the draw");
        let bytes: u64 = p.params.iter().map(|d| d.shape.iter().map(|x| *x as u64).product::<u64>() * d.dtype.width() as u64).sum();
        assert!(work.artifact_bytes >= bytes, "{name}: every param's bytes, at least once");
        assert!(work.ops_supported);
    }
}
