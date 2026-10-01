//! **Spec 17 §17.14: the pipeline-claim data-availability golden vectors**
//! (`consensus-vectors/pipeline-da-v1/`).
//!
//! Regenerated from the pure functions every implementation must agree on — the stage tree's
//! arithmetic (leaf hash, Merkle root, stage root), the answers a prover builds and the checks that
//! accept them, the out-of-range proofs, the compact bindings' execution roots, the tags of the units
//! and answers, and the demand's message — and compared byte for byte with the files.
//! `PIPELINE_DA_BLESS=1` rewrites them, which is a change of the semantics and is reviewed as one.

use std::path::PathBuf;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_da_rcore_v1::{PalwDaAnswerV1, PalwDaUnitV1};
use kaspa_consensus_core::palw_gen_step_v1::{PalwGenLeafCoordV1, PalwGenLeafKindV1, palw_gen_stage_root_v1};
use kaspa_consensus_core::palw_pipeline_da_v1::*;
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_consensus_core::tx::TransactionOutpoint;
use serde_json::{Value, json};

fn h(n: u64) -> Hash64 {
    let mut b = [0u8; 64];
    b[..8].copy_from_slice(&n.to_le_bytes());
    b[63] = 0xA5;
    Hash64::from_bytes(b)
}

fn hex(h: &Hash64) -> String {
    h.to_string()
}

fn hexes(v: &[Hash64]) -> Vec<String> {
    v.iter().map(hex).collect()
}

fn bytes_hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn coord(stage: u8, i: u64) -> PalwGenLeafCoordV1 {
    PalwGenLeafCoordV1 {
        stage,
        pos: (i / 3) as u32,
        kind: PalwGenLeafKindV1::Commit { occurrence: 0, node: (i % 3) as u16 },
        tile: (i % 5) as u32,
    }
}

fn lanes(i: u64) -> Vec<u8> {
    (0..(1 + i % 4)).flat_map(|k| ((i * 7 + k) as u32).to_le_bytes()).collect()
}

fn leaves(stage: u8, n: u64) -> Vec<Hash64> {
    (0..n).map(|i| palw_pipeline_leaf_hash_v1(&coord(stage, i), &lanes(i)).expect("whole lanes")).collect()
}

fn trees() -> Value {
    let counts = [0u64, 1, 2, 3, 4, 5, 7, 8, 9, 17, 33];
    let rows: Vec<Value> = counts
        .iter()
        .map(|n| {
            let ls = leaves(2, *n);
            let merkle = palw_pipeline_merkle_root_v1(&ls);
            json!({
                "leaf_count": n,
                "height": palw_pipeline_tree_height_v1(*n),
                "leaf_hashes": hexes(&ls),
                "merkle_root": hex(&merkle),
                "stage_root": hex(&palw_pipeline_stage_root_v1(2, *n, &merkle)),
                "stage_root_is_the_gen_lanes": palw_gen_stage_root_v1(2, &ls) == palw_pipeline_stage_root_v1(2, *n, &merkle),
            })
        })
        .collect();
    json!({
        "definition": "leaf hash = H(key 'misaka-palw/gen/step-leaf/v1', borsh(coord) ‖ LE u32 n ‖ lanes); node = H(key 'misaka-palw/gen/step-node/v1', left ‖ right), an odd last node promoted; stage root = H(key 'misaka-palw/gen/stage-root/v1', [stage] ‖ LE u64 count ‖ M)",
        "stage": 2,
        "coordinates": "PalwGenLeafCoordV1 { stage, pos: i / 3, kind: Commit { occurrence: 0, node: i % 3 }, tile: i % 5 }; lanes: 1 + i % 4 values (i · 7 + k) as LE u32",
        "rows": rows,
    })
}

fn gen_binding(counts: [u64; 3]) -> (PalwPipelineBindingV1, Vec<Vec<Hash64>>) {
    let ls: Vec<Vec<Hash64>> = counts.iter().enumerate().map(|(s, n)| leaves(s as u8, *n)).collect();
    let stage_roots: Vec<Hash64> = ls.iter().enumerate().map(|(s, l)| palw_gen_stage_root_v1(s as u8, l)).collect();
    let binding = PalwPipelineBindingV1::Gen(PalwPipelineGenPartsV1 {
        job_id: h(1),
        class_id: h(2),
        step_leaf_count: counts.iter().sum(),
        stage_roots,
        generated: vec![5, 6, 7],
    });
    (binding, ls)
}

fn answers() -> Value {
    let (binding, ls) = gen_binding([9, 1030, 2]);
    let facts = PalwPipelineClaimFactsV1 { kind: PalwPipelineKindV1::Gen, class_id: h(2), execution_root: binding.execution_root() };
    let mut nodes = Vec::new();
    for (stage, count, level, index) in
        [(0u8, 9u64, 1u8, 0u64), (0, 9, 4, 0), (0, 9, 2, 1), (1, 1030, 11, 0), (1, 1030, 10, 1), (1, 1030, 3, 100)]
    {
        let l = &ls[stage as usize];
        let (frontier, siblings) = palw_pipeline_node_parts_v1(l, level, index).expect("a node");
        let d = PalwPipelineStepNodeDisclosureV1 {
            version: PALW_PIPELINE_DA_VERSION_V1,
            binding: binding.clone(),
            stage_leaf_count: count,
            frontier: frontier.clone(),
            siblings: siblings.clone(),
        };
        check_pipeline_step_node_v1(&facts, stage, level, index, &d).expect("the answer verifies");
        nodes.push(json!({
            "stage": stage, "level": level, "index": index, "stage_leaf_count": count,
            "frontier": hexes(&frontier), "siblings": hexes(&siblings),
            "frontier_span": palw_pipeline_node_frontier_v1(count, level, index).map(|(b, f, e)| json!([b, f, e])),
        }));
    }
    let mut leaf_rows = Vec::new();
    for (stage, count, index) in [(0u8, 9u64, 0u64), (0, 9, 8), (1, 1030, 0), (1, 1030, 1029), (1, 1030, 512), (2, 2, 1)] {
        let l = &ls[stage as usize];
        let siblings = palw_pipeline_leaf_siblings_v1(l, index).expect("a leaf");
        let d = PalwPipelineStepLeafDisclosureV1 {
            version: PALW_PIPELINE_DA_VERSION_V1,
            binding: binding.clone(),
            stage_leaf_count: count,
            coord: coord(stage, index),
            lanes_le: lanes(index),
            siblings: siblings.clone(),
        };
        check_pipeline_step_leaf_v1(&facts, stage, index, &d).expect("the answer verifies");
        leaf_rows.push(json!({
            "stage": stage, "index": index, "stage_leaf_count": count,
            "lanes_le": bytes_hex(&lanes(index)), "leaf_hash": hex(&l[index as usize]), "siblings": hexes(&siblings),
        }));
    }
    let mut out = Vec::new();
    let tree_of = |stage: usize| PalwPipelineStageTreeV1 {
        leaf_count: ls[stage].len() as u64,
        merkle_root: palw_pipeline_merkle_root_v1(&ls[stage]),
    };
    for (unit, stage_tree) in [
        (PalwDaUnitV1::PipelineStepLeaf { stage: 0, index: 9 }, Some(0usize)),
        (PalwDaUnitV1::PipelineStepLeaf { stage: 1, index: 1030 }, Some(1)),
        (PalwDaUnitV1::PipelineStepNode { stage: 0, level: 5, index: 0 }, Some(0)),
        (PalwDaUnitV1::PipelineStepNode { stage: 0, level: 1, index: 5 }, Some(0)),
        (PalwDaUnitV1::PipelineStepLeaf { stage: 3, index: 0 }, None),
    ] {
        let d = PalwPipelineOutOfRangeV1 {
            version: PALW_PIPELINE_DA_VERSION_V1,
            binding: binding.clone(),
            stage_tree: stage_tree.map(tree_of),
        };
        check_pipeline_out_of_range_v1(&facts, &unit, &d).expect("the proof verifies");
        out.push(json!({
            "unit_borsh": bytes_hex(&borsh::to_vec(&unit).unwrap()),
            "stage_tree": d.stage_tree.map(|t| json!({ "leaf_count": t.leaf_count, "merkle_root": hex(&t.merkle_root) })),
        }));
    }
    let PalwPipelineBindingV1::Gen(parts) = &binding else { unreachable!() };
    // The same trees as a tensor claim's binding (an image or an embedding: a digest output): its execution root is in
    // its own domain, so no text binding of these trees is it.
    let tensor = PalwPipelineBindingV1::Tensor(PalwPipelineTensorPartsV1 {
        job_id: h(5),
        class_id: parts.class_id,
        step_leaf_count: parts.step_leaf_count,
        stage_roots: parts.stage_roots.clone(),
        output_root: h(7),
    });
    json!({
        "claim": {
            "kind": "Gen",
            "job_id": hex(&parts.job_id), "class_id": hex(&parts.class_id),
            "step_leaf_count": parts.step_leaf_count, "stage_roots": hexes(&parts.stage_roots),
            "generated": parts.generated,
            "step_root": hex(&binding.step_root()), "execution_root": hex(&binding.execution_root()),
        },
        "tensor_claim": {
            "kind": "Gen",
            "job_id": hex(&h(5)), "class_id": hex(&parts.class_id),
            "step_leaf_count": parts.step_leaf_count, "stage_roots": hexes(&parts.stage_roots),
            "output_root": hex(&h(7)),
            "step_root": hex(&tensor.step_root()), "execution_root": hex(&tensor.execution_root()),
        },
        "nodes": nodes,
        "leaves": leaf_rows,
        "out_of_range": out,
    })
}

fn tags() -> Value {
    let bond = PalwBondKeyV2(TransactionOutpoint { transaction_id: h(9), index: 3 });
    let units = [
        PalwDaUnitV1::PipelineStepLeaf { stage: 1, index: 0x0102_0304_0506_0708 },
        PalwDaUnitV1::PipelineStepNode { stage: 2, level: 9, index: 5 },
    ];
    let accusation = |unit: &PalwDaUnitV1| palw_pipeline_step_accusation_message_v1(h(1), &h(2), unit, &bond);
    let (binding, _) = gen_binding([3, 3, 3]);
    let answer_tags: Vec<Value> = [
        PalwDaAnswerV1::PipelineStepLeaf(Box::new(PalwPipelineStepLeafDisclosureV1 {
            version: 1,
            binding: binding.clone(),
            stage_leaf_count: 3,
            coord: coord(0, 0),
            lanes_le: vec![],
            siblings: vec![],
        })),
        PalwDaAnswerV1::PipelineStepNode(Box::new(PalwPipelineStepNodeDisclosureV1 {
            version: 1,
            binding: binding.clone(),
            stage_leaf_count: 3,
            frontier: vec![],
            siblings: vec![],
        })),
        PalwDaAnswerV1::PipelineStepOutOfRange(Box::new(PalwPipelineOutOfRangeV1 { version: 1, binding, stage_tree: None })),
    ]
    .iter()
    .map(|a| json!(borsh::to_vec(a).unwrap()[0]))
    .collect();
    json!({
        "unit_tags": [5, 6],
        "unit_borsh": units.iter().map(|u| bytes_hex(&borsh::to_vec(u).unwrap())).collect::<Vec<_>>(),
        "answer_tags": answer_tags,
        "accusation_message_domain": String::from_utf8_lossy(PALW_PIPELINE_ACCUSATION_DOMAIN_V1),
        "accusation_context": String::from_utf8_lossy(PALW_PIPELINE_ACCUSATION_MLDSA87_CONTEXT_V1),
        "accusation_messages": units.iter().map(|u| hex(&accusation(u))).collect::<Vec<_>>(),
        "node_depth": PALW_PIPELINE_STEP_NODE_DEPTH_V1,
        "max_stages": PALW_PIPELINE_MAX_STAGES_V1,
    })
}

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("two parents")
        .join("consensus-vectors/pipeline-da-v1")
}

#[test]
fn the_vectors_are_the_pure_functions() {
    let bless = std::env::var("PIPELINE_DA_BLESS").is_ok_and(|v| v == "1");
    let files: Vec<(&str, Value)> = vec![("trees.json", trees()), ("answers.json", answers()), ("tags.json", tags())];
    for (name, value) in files {
        let path = dir().join(name);
        let mut bytes = serde_json::to_vec_pretty(&value).expect("json");
        bytes.push(b'\n');
        if bless {
            std::fs::create_dir_all(dir()).expect("the vectors directory");
            std::fs::write(&path, &bytes).expect("write the vector file");
        } else {
            let on_disk = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e} (PIPELINE_DA_BLESS=1 writes it)", path.display()));
            assert!(on_disk == bytes, "{} differs from the code's (PIPELINE_DA_BLESS=1 rewrites it)", path.display());
        }
    }
}
