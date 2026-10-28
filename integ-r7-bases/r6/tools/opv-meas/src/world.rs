//! `build-world`: the PRODUCER side of a measurement world (not what is timed as the verifier's cost, but recorded). From a real
//! PALWTIR1 container it registers the class on a kernel ledger (OPV mode if the network's interim terms admit it, else the
//! Panel-licensed registration, with the refusal recorded), posts jobs, produces an honest claim and claims that lie at chosen nodes
//! (every committed value is the honest one but the lied node), writes the chain a fresh verifier replays and publishes, as a
//! content-addressed provider tree (`misaka-palw-remote` layout), each claim's public material and the artifact itself.
use crate::util::*;
use crate::wire::*;
use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_route_policy_v1;
use kaspa_consensus_core::palw_panel_free_v1::PalwPanelFreeFenceV1;
use kaspa_consensus_core::tx::TransactionOutpoint;
use kaspa_hashes::Hash64;
use misaka_palw_kernel::descriptor::{KernelDescriptorV1, k2_tir_v1_descriptor, k2_tir_v2_descriptor};
use misaka_palw_kernel::evidence::build_evidence_v1;
use misaka_palw_kernel::hash::Digest;
use misaka_palw_kernel::job::{DecodeRuleV1, KernelClaimV1, KernelJobV1};
use misaka_palw_kernel::ledger::{KernelRouteObjectV1 as K, LedgerEventV1 as E, claim_seal_v1, single_class_id_v1};
use misaka_palw_kernel::mode::VerificationModeV1;
use misaka_palw_kernel::plan::plan_for_tir_program_v1;
use misaka_palw_kernel::public::{MaterialResponseV1, PositionResponseV1, TensorWireV1, program_root_v1};
use misaka_palw_kernel::trace::{ParamCommitmentsV1, TraceV1, WiringV1, derived_mask_v1, tensor_commitment, trace_v1};
use misaka_palw_remote::evidence::{
    ChunkEntryV1, ENCODING_RAW, EVIDENCE_MANIFEST_VERSION, EvidenceManifestV1, chunk_hash_v1, manifest_id_v1, preclaim_id_v1,
};
use misaka_palw_tir::{Prim, Tensor};
use misaka_palw_tir_artifact::PalwTirContainerV1;
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub const CHUNK_BYTES: usize = 1 << 20;
const BILI: u64 = 100_000_000; // SOMPI_PER_KASPA, the legacy constant name for 1 BILI (ADR-0174)

pub fn bond(i: u8) -> Digest {
    [0xA0u8.wrapping_add(i); 64]
}
pub const ACCUSER: Digest = [0x0B; 64];

/// How a manifest binds a claim's public material: roots derivable from public chain data by a fresh verifier.
pub fn roots_for(trace_root: Digest, output_root: Digest, execution_root: Digest) -> misaka_palw_remote::evidence::ClaimRoots {
    misaka_palw_remote::evidence::ClaimRoots {
        network_domain: Hash64::default(),
        trace_root: Hash64::from_bytes(trace_root),
        output_root: Hash64::from_bytes(output_root),
        execution_root: Hash64::from_bytes(execution_root),
        // A constant here: the real claim would commit the chunk count; the equality check is what is exercised.
        trace_chunk_count: 1,
        retention_deadline: 0,
    }
}

/// Write chunks (as the iterator yields them) into a provider tree under `key_hex` and return `(chunks, bytes)`.
pub fn publish_chunks(
    root: &Path,
    key_hex: &str,
    roots: &misaka_palw_remote::evidence::ClaimRoots,
    mut next: impl FnMut() -> Option<Vec<u8>>,
) -> std::io::Result<(u32, u64)> {
    let outpoint = TransactionOutpoint::new(Hash64::default(), 0);
    let preclaim =
        preclaim_id_v1(roots.network_domain, &outpoint, &[0u8; 32], roots.trace_root, roots.output_root, roots.execution_root);
    let tmp = root.join("chunks").join(format!("_tmp_{key_hex}"));
    std::fs::create_dir_all(&tmp)?;
    std::fs::create_dir_all(root.join("claims"))?;
    let (mut entries, mut total) = (Vec::new(), 0u64);
    while let Some(bytes) = next() {
        let i = entries.len() as u32;
        entries.push(ChunkEntryV1 {
            index: i,
            len: bytes.len() as u32,
            hash: chunk_hash_v1(roots.network_domain, preclaim, i, &bytes),
        });
        total += bytes.len() as u64;
        std::fs::write(tmp.join(format!("{i}.chunk")), &bytes)?;
    }
    if entries.is_empty() {
        std::fs::write(tmp.join("0.chunk"), [])?;
        entries.push(ChunkEntryV1 { index: 0, len: 0, hash: chunk_hash_v1(roots.network_domain, preclaim, 0, &[]) });
    }
    let n = entries.len() as u32;
    let manifest = EvidenceManifestV1 {
        version: EVIDENCE_MANIFEST_VERSION,
        network_domain: roots.network_domain,
        preclaim_id: preclaim,
        trace_root: roots.trace_root,
        output_root: roots.output_root,
        execution_root: roots.execution_root,
        trace_chunk_count: roots.trace_chunk_count,
        retention_until_daa: u64::MAX / 2,
        encoding: ENCODING_RAW,
        max_expanded_bytes: total,
        chunks: entries,
    };
    let id = manifest_id_v1(&manifest);
    let dir = root.join("chunks").join(id.to_string());
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::rename(&tmp, &dir)?;
    let path = root.join("claims").join(format!("{key_hex}.manifest"));
    std::fs::write(&path, borsh::to_vec(&manifest).expect("borsh"))?;
    Ok((n, total))
}

fn bump(t: &mut Tensor, at: usize) {
    let at = at.min(t.data.len() - 1);
    let v = t.data[at];
    t.data[at] = if t.dtype.contains(v + 1) { v + 1 } else { v - 1 };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Lie {
    None,
    Early,
    Late,
    Gather,
    Elem,
    Decode,
}

fn parse_lie(s: &str) -> Result<Lie, String> {
    Ok(match s {
        "honest" => Lie::None,
        "early" => Lie::Early,
        "late" => Lie::Late,
        "gather" => Lie::Gather,
        "elem" => Lie::Elem,
        "decode" => Lie::Decode,
        o => return Err(format!("unknown claim kind {o}")),
    })
}

/// `(position, occurrence, node)` of the node a lie edits.
fn locate(program: &misaka_palw_tir::program::TirProgramV1, lie: Lie, positions: u32) -> Option<(u32, u16, u16)> {
    let occ = program.occurrences();
    let nodes = |s: usize| program.blocks[occ[s].0 as usize].nodes.len();
    let prim = |s: usize, n: usize| &program.blocks[occ[s].0 as usize].nodes[n].prim;
    let last_p = positions - 1;
    match lie {
        Lie::None | Lie::Decode => None,
        Lie::Early => {
            (1..occ.len()).find_map(|s| (0..nodes(s)).find(|n| matches!(prim(s, *n), Prim::MatMul)).map(|n| (0, s as u16, n as u16)))
        }
        Lie::Late => {
            let s = occ.len() - 1;
            (0..nodes(s)).rev().find(|n| matches!(prim(s, *n), Prim::MatMul)).map(|n| (last_p, s as u16, n as u16))
        }
        Lie::Gather => (0..occ.len())
            .find_map(|s| (0..nodes(s)).find(|n| matches!(prim(s, *n), Prim::Gather { .. })).map(|n| (0, s as u16, n as u16))),
        Lie::Elem => (1..occ.len())
            .find_map(|s| (0..nodes(s)).find(|n| matches!(prim(s, *n), Prim::Clamp { .. })).map(|n| (last_p, s as u16, n as u16))),
    }
}

/// The public material of a claim: every committed value but the derived ones, `p u32 · s u16 · n u16 · len u32 · TensorWireV1`.
fn da_blob(trace: &TraceV1, mask: &[Vec<bool>], edit: Option<((u32, u16, u16), &Tensor)>) -> Vec<u8> {
    let mut out = Vec::new();
    for (p, pos) in trace.values.iter().enumerate() {
        for (s, occ) in pos.iter().enumerate() {
            for (n, t) in occ.iter().enumerate() {
                if mask[s][n] {
                    continue;
                }
                let t = match edit {
                    Some(((ep, es, en), et)) if ep as usize == p && es as usize == s && en as usize == n => et,
                    _ => t,
                };
                let wire = borsh::to_vec(&TensorWireV1::of(t)).expect("borsh");
                out.extend_from_slice(&(p as u32).to_le_bytes());
                out.extend_from_slice(&(s as u16).to_le_bytes());
                out.extend_from_slice(&(n as u16).to_le_bytes());
                out.extend_from_slice(&(wire.len() as u32).to_le_bytes());
                out.extend_from_slice(&wire);
            }
        }
    }
    out
}

pub fn run(args: &[String]) -> Result<Value, String> {
    let container_path = PathBuf::from(arg(args, "--container").ok_or("--container PATH")?);
    let out = PathBuf::from(arg(args, "--out").ok_or("--out DIR")?);
    let label = arg(args, "--label").unwrap_or_else(|| "class".into());
    let prompt_len: usize = arg(args, "--prompt").unwrap_or_else(|| "3".into()).parse().map_err(|e| format!("--prompt: {e}"))?;
    let max_positions: u32 =
        arg(args, "--max-positions").unwrap_or_else(|| "8".into()).parse().map_err(|e| format!("--max-positions: {e}"))?;
    let kinds: Vec<Lie> = arg(args, "--claims")
        .unwrap_or_else(|| "honest,late,early,gather,elem,decode".into())
        .split(',')
        .map(|s| parse_lie(s.trim()))
        .collect::<Result<_, _>>()?;
    let kind_names: Vec<String> = arg(args, "--claims")
        .unwrap_or_else(|| "honest,late,early,gather,elem,decode".into())
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();
    let want_opv = !flag(args, "--panel");
    let skip_artifact = flag(args, "--skip-artifact-publish");
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let provider = out.join("provider");
    let mut rec = Rec::default();

    let cont = rec.phase("open_container", || (PalwTirContainerV1::open(&container_path).map_err(|e| e.to_string()), 0));
    let cont = cont?;
    let program = cont.program.clone();
    let program_bytes = cont.header.program.clone();
    let root = program_root_v1(&program_bytes);
    let (d, plan): (KernelDescriptorV1, _) = [k2_tir_v1_descriptor(), k2_tir_v2_descriptor()]
        .into_iter()
        .find_map(|d| plan_for_tir_program_v1(&d, &program, root, max_positions).ok().map(|p| (d, p)))
        .ok_or("no K2 descriptor builds a plan for this program")?;

    // The artifact's commitments, one instance at a time (the whole artifact never sits in memory).
    let pc = rec.phase("param_commitments", || {
        let mut pc = ParamCommitmentsV1::default();
        for e in &cont.header.tensors {
            let t = cont.read_tensor(e.param, e.layer).expect("tensor");
            pc.by_instance.insert((e.param, e.layer), tensor_commitment(&t));
        }
        (pc, cont.file_len)
    });

    let ledger_policy = palw_kernel_route_policy_v1(Hash64::default(), Hash64::default());
    let fence = PalwPanelFreeFenceV1::interim_v1(ForkActivation::new(1), vec![]);
    let opv_policy = fence.opv_policy();
    let make_chain = |blocks: Vec<WBlock>| WChain { policy: ledger_policy, opv: Some(opv_policy), blocks };
    let n_claims = kinds.len();
    let n_producers = n_claims.div_ceil(3).max(1) as u8;
    let mut b1 = vec![];
    for i in 0..n_producers {
        b1.push(WTx::Sync { bond: bond(i), collateral: 5_000 * BILI });
    }
    b1.push(WTx::Sync { bond: ACCUSER, collateral: 1_000 * BILI });
    b1.push(WTx::Attest { root: pc.root() });

    let register = |mode: VerificationModeV1| -> (Vec<WTx>, Digest) {
        let class = single_class_id_v1(d.digest(), &program_bytes, &plan, &pc, mode);
        let mut txs = b1.clone();
        let obj = if mode == VerificationModeV1::OptimisticPublicVerification {
            txs.push(WTx::Admit { class });
            K::RegisterClassV2 {
                mode,
                descriptor: d.digest(),
                program_bytes: program_bytes.clone(),
                plan: plan.clone(),
                param_commitments: pc.clone(),
            }
        } else {
            K::RegisterClass {
                descriptor: d.digest(),
                program_bytes: program_bytes.clone(),
                plan: plan.clone(),
                param_commitments: pc.clone(),
            }
        };
        txs.push(WTx::obj(bond(0), &obj));
        (txs, class)
    };

    let mut opv_refusal: Option<String> = None;
    let mut mode = if want_opv { VerificationModeV1::OptimisticPublicVerification } else { VerificationModeV1::PanelLicensed };
    let mut ledger;
    let mut blocks;
    let class;
    loop {
        let (txs, cls) = register(mode);
        let blk = WBlock { daa: 1, txs };
        let chain = make_chain(vec![]);
        let mut l = chain.genesis();
        let ev = l.apply_block(&blk.to_ledger()?);
        if ev.iter().any(|e| matches!(e, E::ClassRegistered { .. })) {
            ledger = l;
            blocks = vec![blk];
            class = cls;
            break;
        }
        let why = ev
            .iter()
            .find_map(|e| if let E::Refused { tx, why } = e { Some(format!("{tx}: {why}")) } else { None })
            .unwrap_or_else(|| format!("{ev:?}"));
        if mode == VerificationModeV1::OptimisticPublicVerification {
            opv_refusal = Some(why);
            mode = VerificationModeV1::PanelLicensed;
            continue;
        }
        return Err(format!("the class does not register even as Panel-licensed: {why}"));
    }

    // The honest trace, once (the producer's real compute on the real artifact).
    let stream: Vec<u32> = (0..prompt_len).map(|i| ((1000 + 37 * i as u32) % program.token_bound.max(2)).max(1)).collect();
    let mask = derived_mask_v1(&program);
    let honest: TraceV1 =
        rec.phase("honest_trace", || (trace_v1(&program, &cont, &stream).expect("the reference evaluator runs the real class"), 0));
    let honest_ev = rec.phase("honest_commitments", || (honest.evidence(), 0));
    let post = (program.occurrences().len() - 1) as u16;
    let logits_t = &honest.values[prompt_len - 1][post as usize][program.logits as usize];
    let token = DecodeRuleV1::Greedy.select(logits_t).ok_or("no logits")?;
    let wrong_token = {
        let mut best: Option<(usize, i128)> = None;
        for (i, v) in logits_t.data.iter().enumerate() {
            if i as u32 != token && best.is_none_or(|(_, b)| *v > b) {
                best = Some((i, *v));
            }
        }
        best.map(|(i, _)| i as u32).unwrap_or(0)
    };
    let row = ledger.classes.get(&class).ok_or("class row")?.clone();
    let wiring = WiringV1::new(&program).map_err(|e| e.to_string())?;

    let mut jobs = vec![];
    let mut seals = vec![];
    let mut commits = vec![];
    let mut claims_json = vec![];
    for (i, (lie, name)) in kinds.iter().zip(&kind_names).enumerate() {
        let producer = bond((i / 3) as u8);
        let job = KernelJobV1 {
            class_binding_id: class,
            prompt: stream.clone(),
            max_new_tokens: 1,
            decode: DecodeRuleV1::Greedy,
            nonce: [i as u8 + 1; 64],
        };
        let loc = locate(&program, *lie, prompt_len as u32);
        let mut ev_c = honest_ev.clone();
        let edited = loc.map(|(p, s, n)| {
            let mut t = honest.values[p as usize][s as usize][n as usize].clone();
            bump(&mut t, 1);
            ev_c.commitments[p as usize][s as usize][n as usize] = tensor_commitment(&t);
            ((p, s, n), t)
        });
        let ev = build_evidence_v1(&wiring, &ev_c, &stream, row.header(class), &row.descriptor, 2)?;
        let generated = vec![if *lie == Lie::Decode { wrong_token } else { token }];
        let claim = KernelClaimV1 { job_id: job.id(), producer_bond: producer, generated, evidence_root: ev.root() };
        let cid = claim.id();
        jobs.push(WTx::obj(bond(0), &K::PostJob { job: job.clone() }));
        seals.push(WTx::obj(producer, &K::SealClaim { producer, job: job.id(), seal: claim_seal_v1(&cid) }));
        commits
            .push(WTx::obj(producer, &K::CommitClaim { claim: claim.clone(), evidence: ev, commitments: ev_c.commitments.clone() }));
        // Publish this claim's public material.
        let blob = rec.phase(&format!("da_blob_{name}"), || {
            let b = da_blob(&honest, &mask, edited.as_ref().map(|(l, t)| (*l, t)));
            let n = b.len() as u64;
            (b, n)
        });
        let roots = roots_for(cid, cid, job.id());
        let mut chunks = blob.chunks(CHUNK_BYTES).map(<[u8]>::to_vec);
        let (nchunks, bytes) = rec.phase(&format!("publish_da_{name}"), || {
            let r = publish_chunks(&provider, &hex(&cid), &roots, || chunks.next()).expect("publish");
            (r, r.1)
        });
        if i == 0 {
            // A position's whole response (what a producer serves on a demand), for the disclosure measurement.
            let resp: Vec<Vec<MaterialResponseV1>> = honest.values[0]
                .iter()
                .enumerate()
                .map(|(s, o)| {
                    o.iter()
                        .enumerate()
                        .map(
                            |(n, t)| {
                                if mask[s][n] { MaterialResponseV1::Omitted } else { MaterialResponseV1::Whole(TensorWireV1::of(t)) }
                            },
                        )
                        .collect()
                })
                .collect();
            let bytes = borsh::to_vec(&PositionResponseV1 { values: resp, inputs: vec![] }).expect("borsh");
            std::fs::write(out.join(format!("respond-{}-p0.bin", hex(&cid))), &bytes).map_err(|e| e.to_string())?;
        }
        claims_json.push(json!({
            "name": name, "claim_hex": hex(&cid), "job_hex": hex(&job.id()), "producer_hex": hex(&producer), "lie": format!("{lie:?}"),
            "lie_at": loc.map(|(p, s, n)| json!([p, s, n])), "positions": prompt_len, "da_bytes": bytes, "da_chunks": nchunks,
        }));
    }

    // Blocks 2.. : jobs, seals, commits.
    let b2 = WBlock { daa: 2, txs: jobs };
    let b3 = WBlock { daa: 3, txs: seals };
    let b4 = WBlock { daa: 4, txs: commits };
    let mut refused = vec![];
    for b in [&b2, &b3, &b4] {
        let ev = ledger.apply_block(&b.to_ledger()?);
        for e in &ev {
            if let E::Refused { tx, why } = e {
                refused.push(format!("daa {}: {tx}: {why}", b.daa));
            }
        }
        blocks.push(b.clone());
    }
    if !refused.is_empty() {
        return Err(format!("the world's chain refused: {refused:?}"));
    }

    // The artifact, as a provider tree entry keyed by the class id.
    let mut artifact_chunks = 0u32;
    if !skip_artifact {
        let mut f = std::fs::File::open(&container_path).map_err(|e| e.to_string())?;
        let cid = class;
        let roots = roots_for(cid, cid, cid);
        let (n, _bytes) = rec.phase("publish_artifact", || {
            let r = publish_chunks(&provider, &hex(&cid), &roots, || {
                let mut buf = vec![0u8; CHUNK_BYTES];
                let mut got = 0;
                while got < CHUNK_BYTES {
                    let k = f.read(&mut buf[got..]).expect("read");
                    if k == 0 {
                        break;
                    }
                    got += k;
                }
                buf.truncate(got);
                (got > 0).then_some(buf)
            })
            .expect("publish artifact");
            (r, r.1)
        });
        artifact_chunks = n;
    }

    let chain = make_chain(blocks);
    let chain_bytes = borsh::to_vec(&chain).expect("borsh");
    std::fs::write(out.join("chain.bin"), &chain_bytes).map_err(|e| e.to_string())?;
    let world = json!({
        "label": label, "container": container_path.display().to_string(), "container_bytes": cont.file_len,
        "class_hex": hex(&class), "mode": format!("{mode:?}"), "opv_refusal": opv_refusal, "max_positions": max_positions, "prompt_len": prompt_len,
        "artifact_chunks": artifact_chunks, "chain_bytes": chain_bytes.len(), "claims": claims_json,
        "program_bytes": program_bytes.len(), "committed_nodes_per_position": honest.values[0].iter().map(Vec::len).sum::<usize>(),
        "descriptor": format!("{:?}", d.digest()[..8].to_vec()),
    });
    std::fs::write(out.join("world.json"), serde_json::to_vec_pretty(&world).unwrap()).map_err(|e| e.to_string())?;
    let _ = std::io::stdout().flush();
    Ok(json!({"cmd": "build-world", "world": world, "phases": rec.phases, "peak_rss_bytes": rusage().2, "load": loadavg()}))
}
