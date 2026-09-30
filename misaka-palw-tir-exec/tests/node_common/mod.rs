//! Shared by the node-feature tests: the program set (the golden program vectors, the corpus
//! models of `misaka-palw-tir/tests`, and a program whose committed nodes carry `H`), layouts and
//! job contexts.
#![allow(dead_code)]

#[path = "../../../misaka-palw-tir/tests/common/mod.rs"]
pub mod tircommon;

use std::path::PathBuf;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_commitment_v1};
use kaspa_consensus_core::palw_step_refute::{PALW_LOGITS_TILE_LANES, flat_logits_scheme_id_v1, tiled_logits_scheme_id_v1};
use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_LAYOUT_VERSION_V1, PalwTirLayoutV1};
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;
use misaka_palw_tir::{DType, MapParams, Tensor, TirProgramV1};

pub fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex")).collect()
}

/// The program under the flat logits scheme (the vectors' programs name none).
pub fn flat(mut p: TirProgramV1) -> TirProgramV1 {
    p.logits_scheme_id = flat_logits_scheme_id_v1().as_bytes();
    p
}

/// The program under the tiled logits scheme.
pub fn tiled(mut p: TirProgramV1) -> TirProgramV1 {
    p.logits_scheme_id = tiled_logits_scheme_id_v1().as_bytes();
    p
}

pub fn programs() -> Vec<(String, TirProgramV1, MapParams)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../consensus-vectors/tir-v1/programs");
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir).expect("vectors").map(|e| e.unwrap().path()).collect();
    files.sort();
    let mut out: Vec<(String, TirProgramV1, MapParams)> = files
        .into_iter()
        .map(|path| {
            let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            let program = TirProgramV1::decode_canonical(&unhex(v["program_borsh_hex"].as_str().unwrap())).expect("canonical");
            let mut params = MapParams::default();
            for p in v["params"].as_array().unwrap() {
                let j = p["param"].as_u64().unwrap() as u16;
                let layer = p["layer"].as_u64().map(|l| l as u16);
                let d = &program.params[j as usize];
                let shape: Vec<usize> = d.shape.iter().map(|x| *x as usize).collect();
                params
                    .tensors
                    .insert((j, layer), Tensor::from_le_bytes(d.dtype, &shape, &unhex(p["le_hex"].as_str().unwrap())).unwrap());
            }
            (v["name"].as_str().unwrap().to_string(), flat(program), params)
        })
        .collect();
    use tircommon::models::*;
    for (name, (p, gens)) in [
        ("corpus dense", dense(&[HISTORY_BOUND_V1_SMALL, HISTORY_BOUND_V1_SMALL])),
        ("corpus sliding", dense(&[3, HISTORY_BOUND_V1_SMALL, 3])),
        ("corpus gdn", gdn_program(true)),
        ("corpus mamba2", mamba2_program()),
        ("corpus moe", moe_program()),
    ] {
        let params = materialize(&p, &gens, 99);
        out.push((name.to_string(), flat(p), params));
    }
    out.push(h_program());
    out
}

/// Committed nodes that CARRY `H` (a windowed history summed per position): both halves of F4's
/// closed form, under three layer occurrences.
pub fn h_program() -> (String, TirProgramV1, MapParams) {
    use misaka_palw_tir::builder::ProgramBuilder;
    use misaka_palw_tir::{Ref as R, TensorType};
    let mut pb = ProgramBuilder::new(16, HISTORY_BOUND_V1_SMALL);
    let emb = pb.param("embed", DType::I8, &[16, 4], false);
    let hist = pb.hist_state("rows", DType::I8, &[4], 5, true);
    let carry = TensorType::fixed(DType::I8, &[4]);
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let x = b.gather(emb, R::Input(0), 0, 0);
        b.finish(&[x])
    };
    let layer = {
        let mut b = pb.block("layer", vec![carry.clone()]);
        let window = b.hist_append(hist, R::CarryIn(0));
        let wide = b.cast(window, DType::I32);
        let wide = b.commit(wide);
        let sum = b.reduce_sum(wide, 0, DType::I32);
        let sum = b.reshape_fixed(sum, &[4]);
        let y = b.clamp(sum, -128, 127, DType::I8);
        b.finish(&[y])
    };
    let (post, logits) = {
        let mut b = pb.block("post", vec![carry]);
        let l = b.cast(R::CarryIn(0), DType::I32);
        let l = b.commit(l);
        let R::Node(i) = l else { unreachable!() };
        (b.finish(&[]), i)
    };
    let program = pb.finish(pre, vec![layer, layer, layer], post, logits);
    let mut params = MapParams::default();
    let data: Vec<i128> = (0..64).map(|i| ((i * 37 + 11) % 256) as i128 - 128).collect();
    params.tensors.insert((0, None), Tensor::new(DType::I8, vec![16, 4], data).unwrap());
    ("h-window-sum".to_string(), flat(program), params)
}

/// A layout of `p`: ragged commit tiles from `seed`, checkpoint interval `c`, history tile `h` —
/// and, under the tiled logits scheme, the logits node at the scheme's 4,096 lanes (F4's rule).
pub fn layout(p: &TirProgramV1, seed: u32, c: u32, h: u32, max_context: u32) -> PalwTirLayoutV1 {
    let tiled = Hash64::from_bytes(p.logits_scheme_id) == tiled_logits_scheme_id_v1();
    let mut commit_tiles = Vec::new();
    let mut k = 0u32;
    for (bi, b) in p.blocks.iter().enumerate() {
        for (ni, n) in b.nodes.iter().enumerate() {
            if !n.commit {
                continue;
            }
            let logits = bi == p.schedule.post as usize && ni == p.logits as usize;
            commit_tiles.push(if logits && tiled { PALW_LOGITS_TILE_LANES as u32 } else { 4 + (k.wrapping_mul(seed) % 6) });
            k += 1;
        }
    }
    PalwTirLayoutV1 {
        version: PALW_TIR_LAYOUT_VERSION_V1,
        max_context,
        checkpoint_interval: c,
        h_tile: h,
        commit_tiles,
        state_tiles: p.states.iter().enumerate().map(|(j, _)| 4 + ((j as u32 * seed) % 3)).collect(),
    }
}

/// A well-formed job context for `p`'s class: the trace scheme its logits scheme pins (the flat
/// scheme runs under the v2 trace id, as the integer families' contexts do), and the prompt
/// committed under `form`.
pub fn job(
    p: &TirProgramV1,
    prefill: u32,
    decode: u32,
    class_id: Hash64,
    prompt: &[u32],
    form: PalwPromptIdsFormV1,
) -> PalwJobContextV2 {
    let z = Hash64::from_bytes([0u8; 64]);
    let tiled = Hash64::from_bytes(p.logits_scheme_id) == tiled_logits_scheme_id_v1();
    PalwJobContextV2 {
        version: 2,
        network_id: b"testnet-12".to_vec(),
        job_id: Hash64::from_bytes([prefill as u8 ^ 0x5A; 64]),
        job_nullifier: z,
        assignment_id: z,
        execution_seed: [decode as u8; 32],
        model_profile_id: z,
        runtime_manifest_hash: z,
        runtime_class_id: z,
        shape_profile_id: class_id,
        trace_scheme_id: if tiled { tiled_logits_scheme_id_v1() } else { kaspa_consensus_core::palw_v2::trace_scheme_id_v2() },
        cu_ruleset_id: z,
        tokenizer_id: z,
        prompt_token_ids_hash: prompt_token_ids_commitment_v1(form, prompt).expect("a short prompt commits"),
        declared_prefill_tokens: prefill,
        exact_decode_tokens: decode,
        max_context_tokens: 1024,
    }
}
