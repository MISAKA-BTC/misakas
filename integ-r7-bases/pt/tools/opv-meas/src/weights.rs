//! `weights`: the per-PARAMETER work of a fresh verifier, run STREAMING (one tensor at a time, no whole-artifact cache), so it can be
//! measured on every real artifact whatever the host's RAM. For each artifact tensor it times
//!
//! * `read`    — file → `i128` tensor (`PalwTirContainerV1::read_tensor`), once;
//! * `commit`  — `tensor_commitment` (the dual-root Merkle commitment the verifier authenticates a weight against), once. The
//!   reference verifier does this TWICE per weight (`StageMaterial::param`, then `Ctx::authentic`); the report carries both;
//! * `project` — the Freivalds projection `W r` of a rank-2 weight, `reps` repetitions, over GF(2^127 - 1) (the field unit cost the
//!   `micro` command measures). The reference check projects every rank-2 parameter element once per repetition (a `qwen25-0.5b`
//!   whole-claim check counts `field_mults = 2 x params`), so the projection counts every element of a rank-2 tensor.
//!
//! The sum is the part of a whole-claim check that scales with the artifact and not with the number of positions. It is a MODEL of
//! the reference verifier's weight pipeline (the verifier also clones a cached tensor on each use, which this does not time); the
//! `verify` command measures the real thing where the host's RAM allows it, and the two are compared in the measurement note.
use crate::util::*;
use misaka_palw_kernel::field::Fp;
use misaka_palw_kernel::trace::tensor_commitment;
use misaka_palw_tir_artifact::PalwTirContainerV1;
use serde_json::{Value, json};
use std::path::Path;
use std::time::Instant;

fn cpu() -> f64 {
    let (u, s, _) = rusage();
    u + s
}

pub fn run(args: &[String]) -> Result<Value, String> {
    let path = arg(args, "--container").ok_or("--container PATH")?;
    let label = arg(args, "--label").unwrap_or_else(|| path.clone());
    let reps: usize = arg(args, "--reps").unwrap_or_else(|| "2".into()).parse().map_err(|e| format!("--reps: {e}"))?;
    // Stop after this many parameters (0 = all): a quick calibration on a prefix of the artifact.
    let limit: u64 = arg(args, "--limit-params").unwrap_or_else(|| "0".into()).parse().map_err(|e| format!("--limit-params: {e}"))?;
    if let Some(g) = arg(args, "--rss-cap-gb") {
        start_rss_watchdog((g.parse::<f64>().map_err(|e| e.to_string())? * (1u64 << 30) as f64) as u64);
    }
    let c = PalwTirContainerV1::open(Path::new(&path)).map_err(|e| e.to_string())?;
    let load0 = loadavg();
    let (mut n_params, mut n_rank2, mut tensors) = (0u64, 0u64, 0u32);
    let (mut read_wall, mut read_cpu, mut commit_wall, mut commit_cpu, mut proj_wall, mut proj_cpu) =
        (0f64, 0f64, 0f64, 0f64, 0f64, 0f64);
    let mut biggest = 0usize;
    let mut sink = 0u128;
    let wall0 = Instant::now();
    let mut st = 0x9E3779B97F4A7C15u64;
    for e in &c.header.tensors {
        if limit > 0 && n_params >= limit {
            break;
        }
        let (c0, w0) = (cpu(), Instant::now());
        let t = c.read_tensor(e.param, e.layer).map_err(|x| x.to_string())?;
        read_wall += w0.elapsed().as_secs_f64();
        read_cpu += cpu() - c0;
        let len = t.data.len();
        biggest = biggest.max(len);
        n_params += len as u64;
        tensors += 1;

        let (c0, w0) = (cpu(), Instant::now());
        sink ^= tensor_commitment(&t)[0] as u128;
        commit_wall += w0.elapsed().as_secs_f64();
        commit_cpu += cpu() - c0;

        if t.shape.len() == 2 {
            n_rank2 += len as u64;
            let (k, nn) = (t.shape[0], t.shape[1]);
            let (c0, w0) = (cpu(), Instant::now());
            for _ in 0..reps {
                // A challenge vector as the sampler would produce it (the cost of drawing it is a hash per word and is not counted).
                let r: Vec<Fp> = (0..nn)
                    .map(|_| {
                        st ^= st << 13;
                        st ^= st >> 7;
                        st ^= st << 17;
                        Fp::new((st as u128) << 63 | (st as u128 >> 3))
                    })
                    .collect();
                let mut acc = 0u128;
                for kk in 0..k {
                    let row = &t.data[kk * nn..(kk + 1) * nn];
                    let d = Fp::dot(row.iter().map(|v| Fp::from_i128(*v)), r.iter().copied());
                    acc ^= d.value();
                }
                sink ^= acc;
            }
            proj_wall += w0.elapsed().as_secs_f64();
            proj_cpu += cpu() - c0;
        }
    }
    std::hint::black_box(sink);
    let p = n_params.max(1) as f64;
    Ok(json!({
        "cmd": "weights", "label": label, "container_bytes": c.file_len, "tensors": tensors, "params": n_params, "rank2_params": n_rank2,
        "reps": reps, "biggest_tensor_elements": biggest, "wall_s": wall0.elapsed().as_secs_f64(),
        "read": {"wall_s": read_wall, "cpu_s": read_cpu, "cpu_ns_per_param": read_cpu * 1e9 / p},
        "commit_once": {"wall_s": commit_wall, "cpu_s": commit_cpu, "cpu_ns_per_param": commit_cpu * 1e9 / p},
        "project": {"wall_s": proj_wall, "cpu_s": proj_cpu, "field_mults": n_rank2 * reps as u64, "cpu_ns_per_param": proj_cpu * 1e9 / p},
        "model_cpu_s_commit_twice": read_cpu + 2.0 * commit_cpu + proj_cpu,
        "model_cpu_ns_per_param_commit_twice": (read_cpu + 2.0 * commit_cpu + proj_cpu) * 1e9 / p,
        "load_before": load0, "load_after": loadavg(), "peak_rss_bytes": rusage().2,
    }))
}
