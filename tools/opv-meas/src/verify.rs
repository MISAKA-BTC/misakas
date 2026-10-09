//! `verify`: a FRESH outsider verifier, end to end, as a new process. It holds nothing of the producer's: it rebuilds the kernel
//! ledger by replaying the chain's blocks (every object through the strict wire decode), fetches the claim's public material and
//! the artifact from a provider (a directory, or the reference HTTP provider of `misaka-palw-remote` on localhost) with every chunk
//! checked against a manifest that agrees with the claim, runs `OutsiderV1::check` (the kernel's own fresh-verifier path, the one
//! the real-node E2E uses), assembles the filing and has a node replica's court accept (or dismiss) it.
use crate::util::*;
use crate::wire::*;
use crate::world::{ACCUSER, roots_for};
use kaspa_hashes::Hash64;
use misaka_palw_kernel::hash::Digest;
use misaka_palw_kernel::ledger::{
    AuthV1, ClaimBodyV1, KernelLedgerV1, KernelRouteObjectV1 as K, LedgerBlockV1, LedgerEventV1 as E, LedgerTxV1, OutsiderFindingV1,
    OutsiderV1, ProsecutionV1, PublicArtifactV1, PublicSourceV1,
};
use misaka_palw_kernel::public::{FaultProofWireV1, FreshVerifierV1, TensorWireV1};
use misaka_palw_kernel::trace::{ParamCommitmentsV1, tensor_commitment};
use misaka_palw_kernel::verify::{MaterialV1, ScopeV1, ScopeVerdictV1};
use misaka_palw_remote::evidence::{ChunkProvider, ManifestLimits, fetch_chunk_any};
use misaka_palw_remote::transport::{EvidenceProvider, HttpProvider, fetch_claim_material_any};
use misaka_palw_tir::Tensor;
use misaka_palw_tir_artifact::PalwTirContainerV1;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

struct Da(BTreeMap<(u32, u16, u16), Vec<u8>>);

impl PublicSourceV1 for Da {
    fn node(&self, stage: u8, p: u32, s: u16, n: u16) -> Option<Tensor> {
        if stage != 0 {
            return None;
        }
        borsh::from_slice::<TensorWireV1>(self.0.get(&(p, s, n))?).ok()?.decode().ok()
    }
}

struct Art(PalwTirContainerV1);

impl PublicArtifactV1 for Art {
    fn param(&self, program: u16, index: u16, layer: Option<u16>) -> Option<Tensor> {
        if program != 0 { None } else { self.0.read_tensor(index, layer).ok() }
    }
}

/// `StageMaterial` of the kernel's outsider, restated over the public pieces so the fresh verifier can be driven directly (and its
/// cost counters read): node values from the served map, params from the artifact and authenticated against the registered commitments.
struct Mat<'a> {
    da: &'a Da,
    art: &'a Art,
    commitments: &'a ParamCommitmentsV1,
}

impl MaterialV1 for Mat<'_> {
    fn node_value(&self, p: u32, s: u16, n: u16) -> Option<Tensor> {
        self.da.node(0, p, s, n)
    }
    fn param(&self, index: u16, layer: Option<u16>) -> Option<Tensor> {
        let want = self.commitments.by_instance.get(&(index, layer))?;
        let t = self.art.param(0, index, layer)?;
        (tensor_commitment(&t) == *want).then_some(t)
    }
}

fn parse_hex64(s: &str) -> Result<Digest, String> {
    if s.len() != 128 {
        return Err(format!("not a 64-byte hex: {s}"));
    }
    let mut out = [0u8; 64];
    for i in 0..64 {
        out[i] = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

fn parse_da(blob: &[u8], withhold_pos: Option<u32>) -> Result<Da, String> {
    let mut m = BTreeMap::new();
    let mut i = 0usize;
    while i < blob.len() {
        if i + 12 > blob.len() {
            return Err("a truncated record".into());
        }
        let p = u32::from_le_bytes(blob[i..i + 4].try_into().unwrap());
        let s = u16::from_le_bytes(blob[i + 4..i + 6].try_into().unwrap());
        let n = u16::from_le_bytes(blob[i + 6..i + 8].try_into().unwrap());
        let len = u32::from_le_bytes(blob[i + 8..i + 12].try_into().unwrap()) as usize;
        i += 12;
        if i + len > blob.len() {
            return Err("a truncated value".into());
        }
        if withhold_pos != Some(p) {
            m.insert((p, s, n), blob[i..i + len].to_vec());
        }
        i += len;
    }
    Ok(Da(m))
}

fn events_summary(ev: &[E]) -> Vec<String> {
    ev.iter()
        .filter(|e| !matches!(e, E::Settlement(_)))
        .map(|e| match e {
            E::Refused { tx, why } => format!("Refused({tx}: {why})"),
            E::Convicted { slashed, accuser_reward, post_final, .. } => {
                format!("Convicted(slashed {slashed}, accuser_reward {accuser_reward}, post_final {post_final})")
            }
            E::ProofDismissed { why, fee, .. } => format!("ProofDismissed({why}; fee {fee})"),
            other => format!("{other:?}").chars().take(160).collect(),
        })
        .collect()
}

pub fn run(args: &[String]) -> Result<Value, String> {
    if let Some(g) = arg(args, "--rss-cap-gb") {
        start_rss_watchdog((g.parse::<f64>().map_err(|e| e.to_string())? * (1u64 << 30) as f64) as u64);
    }
    let world_dir = PathBuf::from(arg(args, "--world").ok_or("--world DIR")?);
    let name = arg(args, "--claim").unwrap_or_else(|| "honest".into());
    let prov_spec = arg(args, "--provider").ok_or("--provider DIR|http://host:port")?;
    let cache = PathBuf::from(arg(args, "--cache").ok_or("--cache DIR")?);
    let rep: u32 = arg(args, "--rep").unwrap_or_else(|| "0".into()).parse().unwrap_or(0);
    let withhold: Option<u32> = arg(args, "--withhold-pos").map(|s| s.parse().unwrap_or(0));
    let reuse_artifact = flag(args, "--reuse-artifact");
    let skip_court = flag(args, "--skip-court");
    let salt_byte: u8 = arg(args, "--salt").map(|s| s.parse().unwrap_or(0x5A)).unwrap_or(0x5A);
    let load_before = loadavg();
    let wall0 = std::time::Instant::now();
    let mut rec = Rec::default();

    let world: Value =
        serde_json::from_slice(&std::fs::read(world_dir.join("world.json")).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let entry = world["claims"]
        .as_array()
        .and_then(|a| a.iter().find(|c| c["name"] == name))
        .ok_or_else(|| format!("no claim {name}"))?
        .clone();
    let claim_hex = entry["claim_hex"].as_str().unwrap().to_string();
    let class_hex = world["class_hex"].as_str().unwrap().to_string();
    let claim: Digest = parse_hex64(&claim_hex)?;
    let class: Digest = parse_hex64(&class_hex)?;

    // 1. The chain: read, decode, replay (the registration's plan and gate checks are re-run here, as a node would).
    let chain_bytes = rec.phase("read_chain", || {
        let b = std::fs::read(world_dir.join("chain.bin")).expect("chain.bin");
        let n = b.len() as u64;
        (b, n)
    });
    let chain: WChain = rec.phase("decode_chain", || (borsh::from_slice(&chain_bytes).expect("chain"), chain_bytes.len() as u64));
    let blocks: Vec<LedgerBlockV1> =
        rec.phase("decode_route_objects", || (chain.ledger_blocks().expect("blocks"), chain_bytes.len() as u64));
    let ledger = rec.phase("ledger_replay", || {
        let g = chain.genesis();
        (KernelLedgerV1::replay(&g, &blocks), 0)
    });
    let row = ledger.claims.get(&claim).ok_or("the replayed ledger has no such claim")?;
    let job_id = row.job_id;
    if !matches!(row.body, ClaimBodyV1::Program { .. }) {
        return Err("a program claim is expected".into());
    }
    let mode = ledger.mode_of_claim(&claim);

    // 2. The provider.
    let provider: Box<dyn EvidenceProvider> = if prov_spec.starts_with("http://") {
        Box::new(HttpProvider::new(prov_spec.clone()))
    } else {
        Box::new(misaka_palw_remote::evidence::fs::FsProvider::new(PathBuf::from(&prov_spec)))
    };
    let limits = ManifestLimits { max_chunks: 1 << 17, max_chunk_bytes: 1 << 20, max_total_bytes: 1 << 37 };

    // 3. The claim's public material.
    let roots = roots_for(claim, claim, job_id);
    let (blob, report) = {
        let r = rec.phase("fetch_da", || {
            let r = fetch_claim_material_any(&[&*provider], &claim_hex, &roots, &limits, Hash64::default());
            let n = r.as_ref().map(|(b, _)| b.len() as u64).unwrap_or(0);
            (r, n)
        });
        r.map_err(|e| format!("fetch_da: {e}"))?
    };
    let da_chunks = report.served_by.len();
    let da = rec.phase("parse_da", || (parse_da(&blob, withhold).expect("da"), blob.len() as u64));
    let da_bytes = blob.len() as u64;
    drop(blob);

    // 4. The artifact.
    std::fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let art_path = cache.join(format!("{}.palwtir", &class_hex[..16]));
    let mut art_bytes = 0u64;
    if !reuse_artifact {
        let aroots = roots_for(class, class, class);
        art_bytes = rec.phase("fetch_artifact", || {
            let manifest = provider
                .manifest_for(&class_hex)
                .map_err(|e| e.0)
                .and_then(|m| m.ok_or("no artifact manifest".to_string()))
                .expect("artifact manifest");
            manifest.validate_shape(&limits).expect("shape");
            manifest.verify_claim_binding(&aroots).expect("binding");
            let _ = std::fs::remove_file(&art_path);
            let mut f = std::io::BufWriter::with_capacity(4 << 20, std::fs::File::create(&art_path).expect("cache file"));
            let mut total = 0u64;
            let chunk_provider: &dyn ChunkProvider = &*provider;
            for c in &manifest.chunks {
                let (bytes, _, _) = fetch_chunk_any(&manifest, c.index, &[chunk_provider], Hash64::default()).expect("chunk");
                f.write_all(&bytes).expect("write");
                total += bytes.len() as u64;
            }
            f.flush().expect("flush");
            (total, total)
        });
    }
    let art = rec.phase("open_container", || {
        let c = PalwTirContainerV1::open(&art_path).expect("artifact container");
        (Art(c), 0)
    });
    if reuse_artifact {
        art_bytes = art.0.file_len;
    }

    // 5. The check: the kernel's fresh-verifier path.
    let via_fresh = arg(args, "--via").as_deref() == Some("fresh");
    let mut result = json!({});
    let finding = if via_fresh {
        // The same fresh verifier the outsider builds, driven directly: its cost counters, and the filing's assembly timed apart.
        let (record, header) = ledger.public_record(&claim).ok_or("no public record")?;
        let record_bytes = rec.phase("public_record_bytes", || {
            let b = record.to_bytes();
            let n = b.len() as u64;
            (b, n)
        });
        let fresh = rec.phase("fresh_build", || {
            (
                FreshVerifierV1::from_public_bytes(&record_bytes, &ledger.known, header).expect("fresh verifier"),
                record_bytes.len() as u64,
            )
        });
        let class_row = ledger.classes.get(&ledger.claims[&claim].class_binding_id).ok_or("class row")?;
        let mat = Mat { da: &da, art: &art, commitments: &class_row.param_commitments };
        let verdict =
            rec.phase("check_salted", || (fresh.check_salted(&mat, &ScopeV1::WholeClaim, [salt_byte; 64]), da_bytes + art_bytes));
        match verdict {
            ScopeVerdictV1::Pass { positions, probabilistic_checks, error_bits, cost, .. } => {
                result["cost"] = json!({"positions": positions, "probabilistic_checks": probabilistic_checks.to_string(), "error_bits": error_bits,
                    "field_mults": cost.field_mults.to_string(), "opened_bytes": cost.opened_bytes.to_string(), "param_bytes": cost.param_bytes.to_string(),
                    "exact_elements": cost.exact_elements.to_string()});
                Ok(OutsiderFindingV1::Clean)
            }
            ScopeVerdictV1::Fault(proof) => {
                let bytes = rec.phase("assemble_filing_bytes", || {
                    let b = FaultProofWireV1::of(&proof).to_bytes();
                    let n = b.len() as u64;
                    (b, n)
                });
                Ok(OutsiderFindingV1::Prosecute(ProsecutionV1::Kernel(bytes)))
            }
            ScopeVerdictV1::Unavailable { what } => Err(format!("unavailable: {what}")),
            ScopeVerdictV1::EvidenceMalformed { why } => Err(format!("malformed: {why}")),
            ScopeVerdictV1::Inconsistent { why } => Err(format!("inconsistent: {why}")),
        }
    } else {
        rec.phase("check_outsider", || {
            let f = OutsiderV1 { ledger: &ledger, claim, material: &da, artifact: &art, salt: [salt_byte; 64] }.check();
            (f, da_bytes + art_bytes)
        })
    };
    let mut node = ledger;
    let mut filing: Option<ProsecutionV1> = None;
    match finding {
        Ok(OutsiderFindingV1::Clean) => result["finding"] = json!("Clean"),
        Ok(OutsiderFindingV1::Prosecute(p)) => {
            result["finding"] = json!(match &p {
                ProsecutionV1::Kernel(_) => "Prosecute(Kernel)",
                ProsecutionV1::Decode(_) => "Prosecute(Decode)",
                ProsecutionV1::Pipeline(_) => "Prosecute(Pipeline)",
                ProsecutionV1::Spec(_) => "Prosecute(Spec)",
            });
            filing = Some(p);
        }
        Ok(OutsiderFindingV1::Demand(list)) => {
            result["finding"] = json!("Demand");
            result["demand"] = json!(list.iter().take(8).collect::<Vec<_>>());
            result["demand_positions"] = json!(list.len());
        }
        Err(e) => result["finding"] = json!(format!("Err({e})")),
    }

    // 6. The filing: assemble it as a carrier object, and have a node replica's court judge it.
    if let Some(p) = filing {
        if let ProsecutionV1::Kernel(bytes) = &p {
            let (kind, scalar, out_len, openings) = {
                let w: FaultProofWireV1 = borsh::from_slice(bytes).map_err(|e| e.to_string())?;
                (w.kind, w.scalar, w.output.bytes.len(), w.openings.is_some())
            };
            result["fault"] = json!({"kind": kind, "scalar": [scalar.0, scalar.1, scalar.2], "output_value_bytes": out_len, "has_scalar_openings": openings});
            // Re-assemble the filing's canonical bytes (decode, build, encode): the cost of producing the object from a found fault.
            let n = rec.phase("assemble_filing", || {
                let w: FaultProofWireV1 = borsh::from_slice(bytes).expect("wire");
                let proof = w.decode().expect("proof");
                let b = FaultProofWireV1::of(&proof).to_bytes();
                (b.len(), b.len() as u64)
            });
            result["filing_proof_bytes"] = json!(n);
        }
        let obj = K::FileProof { accuser: ACCUSER, claim, proof: p };
        let enc = rec.phase("encode_filing_object", || {
            let e = obj.encode();
            let n = e.len() as u64;
            (e, n)
        });
        result["filing_object_bytes"] = json!(enc.len());
        result["fits_route_object_cap"] =
            json!(enc.len() <= kaspa_consensus_core::palw_kernel_route_v1::PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1);
        let strict = rec.phase("strict_decode_filing", || (K::decode(&enc), enc.len() as u64));
        match strict {
            Err(r) => result["filing_refused_at_wire"] = json!(format!("{}: {}", r.object, r.why)),
            Ok(decoded) if !skip_court => {
                let ev = rec.phase("court_node", || {
                    let b = LedgerBlockV1 {
                        daa: 5,
                        txs: vec![LedgerTxV1::Object { auth: AuthV1 { signer_bond: ACCUSER }, object: decoded }],
                    };
                    (node.apply_block(&b), enc.len() as u64)
                });
                result["court_events"] = json!(events_summary(&ev));
            }
            Ok(_) => {}
        }
    }

    // 7. A demand, if the verifier could not find a value: file it, have the producer serve the position, check again.
    if result["finding"] == "Demand" && flag(args, "--serve") {
        let pos = withhold.unwrap_or(0);
        let resp_path = world_dir.join(format!("respond-{claim_hex}-p0.bin"));
        let resp = std::fs::read(&resp_path).map_err(|e| format!("{}: {e}", resp_path.display()))?;
        let ev1 = rec.phase("demand_node", || {
            let b = LedgerBlockV1 {
                daa: 5,
                txs: vec![LedgerTxV1::Object {
                    auth: AuthV1 { signer_bond: ACCUSER },
                    object: K::FileDemand { demander: ACCUSER, claim, stage: 0, position: pos },
                }],
            };
            (node.apply_block(&b), 0)
        });
        let producer: Digest = parse_hex64(entry["producer_hex"].as_str().unwrap())?;
        let resp_obj = K::Respond { claim, stage: 0, position: pos, bytes: resp.clone() };
        let enc = resp_obj.encode();
        result["response_object_bytes"] = json!(enc.len());
        result["fits_response_object_cap"] =
            json!(enc.len() <= kaspa_consensus_core::palw_kernel_route_v1::PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1);
        let ev2 = rec.phase("respond_node", || {
            let b =
                LedgerBlockV1 { daa: 6, txs: vec![LedgerTxV1::Object { auth: AuthV1 { signer_bond: producer }, object: resp_obj }] };
            (node.apply_block(&b), enc.len() as u64)
        });
        result["demand_events"] = json!(events_summary(&ev1));
        result["respond_events"] = json!(events_summary(&ev2));
        let again = rec.phase("check_after_served", || {
            let f = OutsiderV1 { ledger: &node, claim, material: &da, artifact: &art, salt: [salt_byte; 64] }.check();
            (f, 0)
        });
        result["finding_after_served"] = json!(match again {
            Ok(OutsiderFindingV1::Clean) => "Clean".to_string(),
            Ok(OutsiderFindingV1::Prosecute(_)) => "Prosecute".to_string(),
            Ok(OutsiderFindingV1::Demand(l)) => format!("Demand({})", l.len()),
            Err(e) => format!("Err({e})"),
        });
    }
    if let Some(v) = node.opv_claim_view(&claim) {
        result["claim_state_after"] = json!(format!("{:?}", v.state).chars().take(120).collect::<String>());
    }

    let (u, s, rss) = rusage();
    Ok(json!({
        "cmd": "verify", "via": if via_fresh { "fresh" } else { "outsider" }, "label": world["label"], "claim": name, "mode": format!("{mode:?}"), "provider": if prov_spec.starts_with("http") { "http-localhost" } else { "files" },
        "rep": rep, "salt": salt_byte, "withhold_pos": withhold, "da_bytes": da_bytes, "da_chunks": da_chunks, "artifact_bytes": art_bytes,
        "wall_total_s": wall0.elapsed().as_secs_f64(), "cpu_total_s": u + s, "peak_rss_bytes": rss, "load_before": load_before, "load_after": loadavg(),
        "result": result, "phases": rec.phases,
    }))
}

#[allow(dead_code)]
fn _unused(_: &Path) {}
