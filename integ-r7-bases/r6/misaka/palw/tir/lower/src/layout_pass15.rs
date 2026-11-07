//! Explicit, opt-in history-contraction layout pass. This never edits an
//! artifact, converter default, existing program/root, or shipping descriptor.
//! New canonical bytes and all trace commitments require a new class binding.
//! It preserves each exact integer product and the original k iteration order;
//! it neither changes quantization nor establishes source-checkpoint fidelity.
use misaka_palw_tir::{Dim, Prim, Ref, TensorType, TirProgramV1, program::Node};

pub const LAYOUT_PASS_ID_V15: &str = "misaka.palw.frontend/history-contraction-batch-layout/v15";

#[derive(Clone, Debug)]
pub struct HistoryContractionRewriteV15 {
    pub block: u8,
    pub original_node: u16,
    pub replacement_node: u16,
    pub first_added_node: u16,
}
#[derive(Clone, Debug)]
pub struct HistoryContractionLayoutV15 {
    pub program: TirProgramV1,
    pub rewrites: Vec<HistoryContractionRewriteV15>,
}

/// For rank<=3 A[...,M,H] × B[...,H,N], move M into a broadcast batch:
/// reshape(B^T,[...,1,N,H]) × reshape(A,[...,M,H,1]), then restore [...,M,N].
/// Both reduction operands become contiguous H lines, including grouped-query
/// attention where a naive transpose would leave an H×group strided column.
/// The extra transpose, views, MatMul output, hashing and trace/storage costs
/// still have to be charged. Combined leaf and byte caps may continue to refuse.
pub fn transpose_history_contractions_v15(source: &TirProgramV1) -> Result<HistoryContractionLayoutV15, String> {
    misaka_palw_tir::validate::validate(source).map_err(|e| format!("source program: {e}"))?;
    let mut program = source.clone();
    let mut rewrites = vec![];
    for (bi, old) in source.blocks.iter().enumerate() {
        let mut nodes = vec![];
        let mut map = Vec::<u16>::new();
        let remap = |r: Ref, map: &[u16]| match r {
            Ref::Node(n) => Ref::Node(map[n as usize]),
            r => r,
        };
        for (ni, node) in old.nodes.iter().enumerate() {
            let candidate = match (&node.prim, node.inputs.as_slice()) {
                (Prim::MatMul, [Ref::Node(a), Ref::Node(b)]) => {
                    let (a, b) = (&old.nodes[*a as usize].out, &old.nodes[*b as usize].out);
                    let (ar, br) = (a.shape.len(), b.shape.len());
                    if (2..=3).contains(&ar)
                        && (2..=3).contains(&br)
                        && node.out.shape.len() <= 3
                        && a.shape[ar - 1] == Dim::H
                        && b.shape[br - 2] == Dim::H
                    {
                        Some((a, b))
                    } else {
                        None
                    }
                }
                _ => None,
            };
            if let Some((a, b)) = candidate {
                let first = u16::try_from(nodes.len()).map_err(|_| "new node extent")?;
                let mut perm = (0..b.shape.len() as u8).collect::<Vec<_>>();
                let br = perm.len();
                perm.swap(br - 2, br - 1);
                let bt = TensorType::new(b.dtype, perm.iter().map(|i| b.shape[*i as usize]).collect());
                nodes.push(Node {
                    prim: Prim::Transpose { perm },
                    inputs: vec![remap(node.inputs[1], &map)],
                    out: bt.clone(),
                    commit: false,
                });
                let mut bshape = bt.shape;
                bshape.insert(br - 2, Dim::Fixed(1));
                nodes.push(Node {
                    prim: Prim::Reshape,
                    inputs: vec![Ref::Node(first)],
                    out: TensorType::new(b.dtype, bshape),
                    commit: false,
                });
                let mut ashape = a.shape.clone();
                ashape.push(Dim::Fixed(1));
                nodes.push(Node {
                    prim: Prim::Reshape,
                    inputs: vec![remap(node.inputs[0], &map)],
                    out: TensorType::new(a.dtype, ashape),
                    commit: false,
                });
                let mut mmshape = node.out.shape.clone();
                mmshape.push(Dim::Fixed(1));
                nodes.push(Node {
                    prim: Prim::MatMul,
                    inputs: vec![Ref::Node(first + 1), Ref::Node(first + 2)],
                    out: TensorType::new(node.out.dtype, mmshape),
                    commit: false,
                });
                nodes.push(Node {
                    prim: Prim::Reshape,
                    inputs: vec![Ref::Node(first + 3)],
                    out: node.out.clone(),
                    commit: node.commit,
                });
                map.push(first + 4);
                rewrites.push(HistoryContractionRewriteV15 {
                    block: bi as u8,
                    original_node: ni as u16,
                    replacement_node: first + 4,
                    first_added_node: first,
                });
            } else {
                map.push(u16::try_from(nodes.len()).map_err(|_| "new node extent")?);
                let mut new = node.clone();
                new.inputs = node.inputs.iter().map(|r| remap(*r, &map)).collect();
                nodes.push(new);
            }
        }
        program.blocks[bi].nodes = nodes;
        program.blocks[bi].carry_out = old.carry_out.iter().map(|n| map[*n as usize]).collect();
        if bi as u8 == source.schedule.post {
            program.logits = map[source.logits as usize];
        }
    }
    if program.encode().len() > misaka_palw_tir::program::MAX_PROGRAM_BYTES {
        return Err("transformed program byte cap".into());
    }
    misaka_palw_tir::validate::validate(&program).map_err(|e| format!("transformed program: {e}"))?;
    Ok(HistoryContractionLayoutV15 { program, rewrites })
}
