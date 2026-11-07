//! **A seat's executor conformance** (feature `node`; RFC-0002 Part II §II.9 L2 and §II.7.5 Proposal B): the node's typed executor over
//! the artifact file it holds, run against the reference evaluator on a few positions with the reference reading each param lazily
//! from the container — identical at every position's logits and every commit point on every program of the node suites (the golden
//! vectors, the corpus models, a program whose committed nodes carry `H`), and refusing, by position and commit point, a reference
//! whose weights differ from the executor's by one value.

#![cfg(feature = "node")]

mod node_common;

use std::path::{Path, PathBuf};

use misaka_palw_tir::{MapParams, ParamSource, Tensor, TirProgramV1};
use misaka_palw_tir_exec::node::{
    LazyContainerParams, TIR_CONFORMANCE_POSITIONS_V1, TirArtifactV1, tir_conformance_prompt_v1, tir_executor_conformance_against_v1,
    tir_executor_conformance_of_file_v1,
};
use node_common::{layout, programs};

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let d = std::env::temp_dir().join(format!("tir-conformance-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Scratch(d)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn write(dir: &Path, name: &str, program: &TirProgramV1, params: &MapParams) -> PathBuf {
    let path = dir.join(format!("{}.palwtir", name.replace(' ', "-")));
    let lay = layout(program, 5, 2, 2, 64);
    let mut tensor = |j: u16, l: Option<u16>| -> Result<Vec<u8>, String> {
        params.tensors.get(&(j, l)).map(|t| t.to_le_bytes()).ok_or_else(|| format!("no tensor {j} {l:?}"))
    };
    misaka_palw_tir_artifact::write_container_v1(&path, program, borsh::to_vec(&lay).unwrap(), [2; 64], name.into(), &mut tensor)
        .unwrap_or_else(|e| panic!("{name}: {e}"));
    path
}

#[test]
fn every_program_of_the_node_suites_conforms_streamed_and_sampled() {
    let dir = Scratch::new("all");
    for (name, program, params) in programs() {
        let path = write(&dir.0, &name, &program, &params);
        let got = tir_executor_conformance_of_file_v1(&path, TIR_CONFORMANCE_POSITIONS_V1).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(got.positions, TIR_CONFORMANCE_POSITIONS_V1, "{name}");
        assert!(got.commits > 0, "{name}: commit points were compared");
        if !params.tensors.is_empty() {
            assert!(got.reference_param_reads > 0, "{name}: the reference read its params through the lazy source");
        }
        // Streamed: the reference never held more than one tensor at a time — the largest one, in its i128 form.
        let largest = params.tensors.values().map(|t| t.data.len() * 16).max().unwrap_or(0) as u64;
        assert!(
            got.reference_peak_tensor_bytes <= largest,
            "{name}: held {} bytes at once, the largest tensor is {largest}",
            got.reference_peak_tensor_bytes
        );
    }
}

/// A reference whose params differ from the artifact's by one flipped weight: the gate fires, naming a position.
struct Flipped<'a> {
    inner: LazyContainerParams<'a>,
    target: (u16, Option<u16>),
}

impl ParamSource for Flipped<'_> {
    fn param(&self, index: u16, layer: Option<u16>) -> Option<Tensor> {
        let mut t = self.inner.param(index, layer)?;
        if (index, layer) == self.target {
            let first = t.data[0];
            // The least change that stays inside the dtype: toggle the lowest bit.
            t.data[0] = first ^ 1;
        }
        Some(t)
    }
}

#[test]
fn a_reference_that_differs_by_one_weight_is_refused_by_position() {
    let dir = Scratch::new("flip");
    let mut fired = 0;
    for (name, program, params) in programs() {
        let path = write(&dir.0, &name, &program, &params);
        let artifact = TirArtifactV1::open(&path).expect("opens");
        // Flip the first element of each instance in turn; at least one instance of a model changes what the executor commits.
        let mut refused = false;
        for &(j, layer) in params.tensors.keys() {
            let flipped = Flipped { inner: LazyContainerParams::new(artifact.container()), target: (j, layer) };
            match tir_executor_conformance_against_v1(&artifact, &flipped, TIR_CONFORMANCE_POSITIONS_V1) {
                Ok(_) => {}
                Err(why) => {
                    assert!(why.contains("position") && why.contains("reference evaluator"), "{name}: {why}");
                    refused = true;
                    break;
                }
            }
        }
        if refused {
            fired += 1;
        }
    }
    assert!(fired > 0, "the gate fires on some program of the suites");
}

#[test]
fn the_prompt_is_the_same_for_every_seat() {
    for (_, program, _) in programs() {
        let a = tir_conformance_prompt_v1(program.token_bound, 3);
        assert_eq!(a, tir_conformance_prompt_v1(program.token_bound, 3));
        assert!(a.iter().all(|t| *t < program.token_bound.max(1)));
    }
}

#[test]
fn invalid_position_requests_refuse_before_reference_reads_or_execution() {
    struct NoReads;
    impl ParamSource for NoReads {
        fn param(&self, _: u16, _: Option<u16>) -> Option<Tensor> {
            panic!("invalid context reached parameter acquisition");
        }
    }
    let dir = Scratch::new("exact-context");
    let (name, program, params) = programs().into_iter().next().unwrap();
    let path = write(&dir.0, &name, &program, &params);
    let artifact = TirArtifactV1::open(&path).unwrap();
    for positions in [0, program.history_bound as usize + 1, usize::MAX] {
        let why = tir_executor_conformance_against_v1(&artifact, &NoReads, positions).unwrap_err();
        assert!(why.contains("requested positions"), "{why}");
    }
    assert_eq!(tir_executor_conformance_of_file_v1(&path, 1).unwrap().positions, 1);
}
