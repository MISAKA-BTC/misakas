//! Actual pinned full dense-model execution, independent first-mismatch locator
//! and ordinary9 conviction. Deliberate producer mutation is inaccessible to
//! the locator. This local component trial grants no native eligibility/Final.
use misaka_palw_kernel::{
    element::SegClaimContextV1,
    flat_court5::*,
    flat_court9::*,
    flat_detect9::*,
    flat_point9::*,
    flat_scope9::*,
    hash::Digest,
    job::DecodeRuleV1,
    merkle::{AXIS_COL, AXIS_ROW},
    merkle3::{LeafOpeningV3, TreesV3},
    merkle5::{FlatLeafValueV5, FlatTreeV5},
    seg::{SegmentedCommitmentsV1, prompt_root_of_ids_v1},
    trace::{ParamCommitmentsV1, SourceV1, WiringV1},
};
use misaka_palw_tir::{Tensor, TirProgramV1};
use misaka_palw_tir_exec::{NodeValue, StepSink, TirExecutor, node::TirArtifactV1};
use sha2::{Digest as _, Sha256};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::Path,
};
const OWN_POSITION_CAP: u128 = 256 << 20;
const OWN_PARAMETER_CAP: usize = 64 << 20;
fn hash(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut b = vec![0; 1 << 20];
    loop {
        let n = f.read(&mut b)?;
        if n == 0 {
            break;
        }
        h.update(&b[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}
fn bounded(path: &Path, max: usize) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut b = vec![];
    std::fs::File::open(path)?.take(max as u64 + 1).read_to_end(&mut b)?;
    if b.len() > max {
        return Err("metadata bytes exceed cap".into());
    }
    Ok(b)
}
fn coordinates(program: &TirProgramV1) -> Result<BTreeMap<(u8, Option<u16>), u16>, String> {
    let mut m = BTreeMap::new();
    for (s, key) in program.occurrences().iter().enumerate() {
        if m.insert(*key, s as u16).is_some() {
            return Err("this example requires distinct block/layer occurrence coordinates".into());
        }
    }
    Ok(m)
}
fn blank<T>(p: &TirProgramV1) -> Vec<Vec<Option<T>>> {
    p.occurrences().iter().map(|(b, _)| (0..p.blocks[*b as usize].nodes.len()).map(|_| None).collect()).collect()
}
struct Producer<'a> {
    p: &'a TirProgramV1,
    map: BTreeMap<(u8, Option<u16>), u16>,
    nodes: Vec<Vec<Option<Digest>>>,
    mutation: (u16, u16),
    wrong: Option<Tensor>,
    logits: Option<Tensor>,
    error: Option<String>,
    seen: usize,
}
impl StepSink for Producer<'_> {
    fn every_node(&self) -> bool {
        true
    }
    fn node(&mut self, v: &NodeValue<'_>) {
        let s = self.map[&(v.block, v.layer)];
        let n = v.node;
        if v.data.len() as u128 * 16 > OWN_POSITION_CAP {
            self.error = Some("producer tensor payload preflight".into());
            return;
        }
        let mut t = v.to_tensor();
        if (s, n) == self.mutation {
            t.data[0] += 1;
            self.wrong = Some(t.clone());
        }
        if s as usize + 1 == self.nodes.len() && n == self.p.logits {
            self.logits = Some(t.clone())
        }
        self.seen += 1;
        match flat_profile_commitment_v5(&t) {
            Ok(root) => self.nodes[s as usize][n as usize] = Some(root),
            Err(e) => self.error = Some(e.to_string()),
        }
    }
}
struct Finder<'a> {
    map: BTreeMap<(u8, Option<u16>), u16>,
    public: &'a SegmentedCommitmentsV1,
    own: Vec<Vec<Option<Tensor>>>,
    first: Option<(u16, u16)>,
    prior_matches: usize,
    seen: usize,
    bytes: u128,
    error: Option<String>,
}
impl StepSink for Finder<'_> {
    fn every_node(&self) -> bool {
        true
    }
    fn node(&mut self, v: &NodeValue<'_>) {
        let s = self.map[&(v.block, v.layer)];
        self.bytes += v.data.len() as u128 * 16;
        if self.bytes > OWN_POSITION_CAP {
            self.error = Some("own position payload cap".into());
            return;
        }
        let t = v.to_tensor();
        self.seen += 1;
        match flat_profile_commitment_v5(&t) {
            Ok(root) => {
                if self.public.node_opening(0, s, v.node).map(|o| o.commitment) != Some(root) {
                    self.first.get_or_insert((s, v.node));
                } else if self.first.is_none() {
                    self.prior_matches += 1;
                }
                self.own[s as usize][v.node as usize] = Some(t);
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }
}
fn output_proof(t: &Tensor, node: misaka_palw_kernel::seg::NodeOpeningV1, e: u64) -> Result<FlatOperandV5, String> {
    let mut out = FlatOperandV5 { node: Some(node), ..Default::default() };
    if t.data.len() >= 64 {
        let tree = FlatTreeV5::of(t).map_err(|e| e.to_string())?;
        out.flat.push(FlatLeafValueV5::of(t, &tree, tree.layout().leaf_of(e).ok_or("output index")?).ok_or("output leaf")?);
    } else {
        let l = misaka_palw_kernel::merkle3::LayoutV3::of(&t.shape);
        let (row, tile, _) = l.row_leaf_of(e).ok_or("output index")?;
        out.legacy.push(LeafOpeningV3::of_trees(t, &TreesV3::of(t), AXIS_ROW, row, tile).ok_or("output row")?)
    }
    Ok(out)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 4 {
        return Err("usage: check-real-point-court9 <artifact> <param-commitments.borsh> <predeclared fidelity protocol.json> <fresh proof.bin>".into());
    }
    let policy: serde_json::Value = serde_json::from_slice(&bounded(Path::new(&args[2]), 64 << 10)?)?;
    if policy["schema"] != "misaka.rfc02.independent-source-fidelity-protocol.v1" {
        return Err("wrong source policy".into());
    }
    let pin = policy["artifact_sha256"].as_str().ok_or("artifact pin")?;
    let artifact_path = Path::new(&args[0]);
    if hash(artifact_path)? != pin {
        return Err("artifact identity before run".into());
    }
    let pcbytes = bounded(Path::new(&args[1]), 64 << 20)?;
    let pcsha = Sha256::digest(&pcbytes).iter().map(|b| format!("{b:02x}")).collect::<String>();
    if pcsha != "a45721593036993c53eb29ad1526906ca190fcdf7d7272fbf7aa3f0b62d19ef1" {
        return Err("pinned Qwen parameter commitment identity".into());
    }
    let pc: ParamCommitmentsV1 = borsh::from_slice(&pcbytes)?;
    let artifact = TirArtifactV1::open(artifact_path)?;
    let p = &artifact.plan().program;
    let psha = Sha256::digest(p.encode()).iter().map(|b| format!("{b:02x}")).collect::<String>();
    if psha != "16a0de98fe52cdf47364392551b15bb85a7aad76acd8b14853b1803074bca8f1" {
        return Err("pinned Qwen canonical program identity".into());
    }
    let scope = flat_resource_scope_v9(p, 32, None).map_err(|e| format!("{e:?}"))?;
    check_flat_resource_scope_v9(p, 32, None, &scope).map_err(|e| format!("{e:?}"))?;
    if scope.reference_tensor_bytes_per_position > OWN_POSITION_CAP {
        return Err("reference position payload preflight".into());
    }
    let token = u32::try_from(policy["fed_tokens"][0][0].as_u64().ok_or("predeclared first fed id")?)?;
    if token >= p.token_bound {
        return Err("job id bound".into());
    }
    let tokens = [token];
    let map = coordinates(p)?;
    // Mutation belongs solely to the producer. Independent locator is passed only
    // the public commitment directory and its independently mapped model/job.
    let started = std::time::Instant::now();
    let mut producer =
        Producer { p, map: map.clone(), nodes: blank(p), mutation: (1, 27), wrong: None, logits: None, error: None, seen: 0 };
    TirExecutor::new(artifact.plan(), artifact.params())?.step(token, &mut producer)?;
    if let Some(e) = producer.error {
        return Err(e.into());
    }
    let generated =
        [DecodeRuleV1::Greedy.select(producer.logits.as_ref().ok_or("missing whole-model logits")?).ok_or("greedy logits")?];
    let directory = producer
        .nodes
        .into_iter()
        .map(|s| s.into_iter().map(|n| n.ok_or("missing producer node")).collect::<Result<Vec<_>, _>>())
        .collect::<Result<Vec<_>, _>>()?;
    let public = SegmentedCommitmentsV1::new(vec![directory]);
    let roots = public.segment_roots();
    let producer_ms = started.elapsed().as_millis();
    let mut finder = Finder { map, public: &public, own: blank(p), first: None, prior_matches: 0, seen: 0, bytes: 0, error: None };
    let independently_opened = TirArtifactV1::open(artifact_path)?;
    TirExecutor::new(independently_opened.plan(), independently_opened.params())?.step(token, &mut finder)?;
    if let Some(e) = finder.error {
        return Err(e.into());
    }
    let (s, n) = finder.first.ok_or("independent replay found no discrepancy")?;
    let base = FlatSegContextV5 {
        profile: flat_trace_profile_id_v5(),
        claim: SegClaimContextV1 {
            program: p,
            params: &pc,
            segment_roots: &roots,
            positions: 1,
            prompt_len: 1,
            prompt_root: prompt_root_of_ids_v1(&tokens),
            inline_prompt: Some(&tokens),
            generated: &generated,
            decode: DecodeRuleV1::Greedy,
            encoder: None,
        },
    };
    let c = FlatContextV9 { scope: &scope, base };
    let point = PointContextV9 {
        court_id: ordinary_point_court_id_v9(),
        base: FlatSegContextV5 {
            profile: c.base.profile,
            claim: SegClaimContextV1 {
                program: p,
                params: &pc,
                segment_roots: &roots,
                positions: 1,
                prompt_len: 1,
                prompt_root: prompt_root_of_ids_v1(&tokens),
                inline_prompt: Some(&tokens),
                generated: &generated,
                decode: DecodeRuleV1::Greedy,
                encoder: None,
            },
        },
    };
    let wrong = producer.wrong.as_ref().ok_or("producer supplied no divergent typed leaf")?;
    let reply = FlatValueV9 {
        version: 9,
        scope: scope.root(),
        value: output_proof(wrong, public.node_opening(0, s, n).ok_or("public node")?, 0)?,
    }
    .to_bytes()?;
    let reply = FlatValueV9::from_bytes(&reply)?;
    let expected = finder.own[s as usize][n as usize].as_ref().ok_or("own detected node")?;
    let element = (0..expected.data.len() as u64)
        .find(|e| {
            authenticated_output_point_v9(&reply.value, expected.dtype, &expected.shape, *e)
                .is_some_and(|v| v != expected.data[*e as usize])
        })
        .ok_or("no authenticated divergent point")?;
    let wiring = WiringV1::new(p)?;
    let node = wiring.node(s, n);
    let sources = (0..node.inputs.len()).map(|i| wiring.input_source(&tokens, 0, s, n, i)).collect::<Result<Vec<_>, _>>()?;
    let mut own = vec![];
    let mut selected_param_instances = vec![];
    for source in &sources {
        own.push(match source {
            SourceV1::Node { position: 0, occurrence, node } => {
                finder.own[*occurrence as usize][*node as usize].as_ref().ok_or("missing own prior value")?.clone()
            }
            SourceV1::Param { index, layer } => {
                let d = &p.params[*index as usize];
                let len = d.shape.iter().try_fold(1usize, |a, b| a.checked_mul(*b as usize)).ok_or("param extent overflow")?;
                if len.checked_mul(16).is_none_or(|b| b > OWN_PARAMETER_CAP) {
                    return Err("own filing parameter payload preflight".into());
                }
                let values = independently_opened.params().get(*index, *layer).ok_or("missing independently mapped param")?.to_i128s();
                let t = Tensor::new(d.dtype, d.shape.iter().map(|d| *d as usize).collect(), values)?;
                if pc.by_instance.get(&(*index, *layer)) != Some(&misaka_palw_kernel::merkle3::tensor_commitment_v3(&t)) {
                    return Err("actual selected parameter differs from registered root".into());
                }
                selected_param_instances.push((*index, *layer));
                t
            }
            _ => return Err("example selected relation needs only independently replayed nodes/registered params".into()),
        })
    }
    let deps = point_dependencies_v9(&point, 0, s, n, element, &own).map_err(|e| format!("{e:?}"))?;
    let mut inputs = vec![];
    for (i, ((source, t), reads)) in sources.iter().zip(&own).zip(&deps.leaves).enumerate() {
        let node = match source {
            SourceV1::Node { position, occurrence, node } => {
                let o = public.node_opening(*position, *occurrence, *node).ok_or("missing own input node opening")?;
                if flat_profile_commitment_v5(t)? != o.commitment {
                    return Err("not first divergent relation".into());
                }
                Some(o)
            }
            _ => None,
        };
        let mut out = FlatOperandV5 { node, ..Default::default() };
        if deps.kinds[i] == PointOperandKindV9::Flat {
            let tree = FlatTreeV5::of(t)?;
            for &(leaf, _) in reads {
                out.flat.push(FlatLeafValueV5::of(t, &tree, leaf).ok_or("own input leaf")?);
            }
        } else {
            let axis = if deps.kinds[i] == PointOperandKindV9::Column { AXIS_COL } else { AXIS_ROW };
            let tree = TreesV3::of(t);
            for &(line, tile) in reads {
                out.legacy.push(LeafOpeningV3::of_trees(t, &tree, axis, line, tile).ok_or("own input tile")?);
            }
        }
        inputs.push(out)
    }
    let f = FlatFaultV9::Ordinary(FlatPointFaultV9 { version: 9, element, output: reply.value, inputs, token: None });
    let wire = f.to_bytes()?;
    let f = FlatFaultV9::from_bytes(&wire)?;
    let bound = flat_fault_court_bounds_v9(&c, &f).map_err(|e| format!("{e:?}"))?;
    let conviction = verify_flat_fault_v9(&c, &f).map_err(|e| format!("{e:?}"))?;
    let mut unavailable = f.clone();
    if let FlatFaultV9::Ordinary(ref mut f) = unavailable {
        f.inputs[0].legacy.clear();
    }
    if verify_flat_fault_v9(&c, &unavailable).is_ok() {
        return Err("missing source material was convicted".into());
    }
    if hash(artifact_path)? != pin {
        return Err("artifact identity after run".into());
    }
    std::fs::OpenOptions::new().write(true).create_new(true).open(&args[3])?.write_all(&wire)?;
    println!(
        "{}",
        serde_json::json!({"schema":"misaka.rfc02.actual-checkpoint-ordinary9-local-trial.v1","artifact_sha256":pin,"canonical_program_sha256":psha,"parameter_map_sha256":pcsha,"parameter_root":misaka_palw_sdk::runtime_pack::commit::hex(&pc.root()),"scope_root":misaka_palw_sdk::runtime_pack::commit::hex(&scope.root()),"admitted_metadata_max_positions":32,"actual_positions":1,"job_tokens":tokens,"producer_nodes":producer.seen,"independent_replay_nodes":finder.seen,"independent_prior_node_matches":finder.prior_matches,"independent_detected_occurrence":s,"independent_detected_node":n,"detected_element":element,"selected_parameter_instances_independently_authenticated":selected_param_instances,"own_reference_tensor_payload_bytes":finder.bytes,"source_artifact_sha256_before_after_match":true,"producer_elapsed_ms":producer_ms,"total_elapsed_ms":started.elapsed().as_millis(),"wire_bytes":wire.len(),"proof_sha256":Sha256::digest(&wire).iter().map(|b|format!("{b:02x}")).collect::<String>(),"court_bounds":{"filing_bytes":bound.filing_bytes,"work":bound.work,"scratch_bytes":bound.scratch_bytes,"operand_leaves":bound.operand_leaves},"conviction":format!("{conviction:?}"),"missing_source_material_dismissed":true,"raw_source_float_fidelity_this_run":false,"full_parameter_map_source_continuity_this_run":false,"native_registration_or_execution_final_or_a6_a8_credit":false,"full_rfc02_complete":false})
    );
    Ok(())
}
