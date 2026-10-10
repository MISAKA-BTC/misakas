//! `micro`: unit costs the class-level numbers are built from — measured here once per host, so a class that cannot be run end to end
//! on this host (the reference verifier holds 16 bytes per parameter) can still be costed from its operation counts.
use crate::util::*;
use misaka_palw_kernel::field::Fp;
use misaka_palw_kernel::trace::tensor_commitment;
use misaka_palw_tir::{DType, Tensor};
use misaka_palw_tir_artifact::PalwTirContainerV1;
use serde_json::{Value, json};
use std::io::Read;
use std::path::Path;
use std::time::Instant;

fn xorshift(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

fn timed<R>(f: impl FnOnce() -> R) -> (R, f64, f64) {
    let t0 = Instant::now();
    let (u0, s0, _) = rusage();
    let r = f();
    let (u1, s1, _) = rusage();
    (r, t0.elapsed().as_secs_f64(), (u1 + s1) - (u0 + s0))
}

pub fn run(args: &[String]) -> Result<Value, String> {
    let reps: usize = arg(args, "--reps").unwrap_or_else(|| "5".into()).parse().unwrap_or(5);
    let load0 = loadavg();
    let mut st = 0x9E3779B97F4A7C15u64;
    let mut out = json!({"cmd": "micro", "load_before": load0, "reps": reps});

    // 1. GF(2^127 - 1) multiply-accumulate (the Freivalds projection's inner loop), ns per multiply.
    let n = 4_000_000usize;
    let a: Vec<Fp> = (0..n).map(|_| Fp::new(xorshift(&mut st) as u128 * 0x1_0000_0001)).collect();
    let b: Vec<Fp> = (0..n).map(|_| Fp::new(xorshift(&mut st) as u128 * 0x1_0000_0003)).collect();
    let mut f = vec![];
    for _ in 0..reps {
        let (r, wall, cpu) = timed(|| Fp::dot(a.iter().copied(), b.iter().copied()));
        std::hint::black_box(r);
        f.push(json!({"wall_ns_per_mult": wall * 1e9 / n as f64, "cpu_ns_per_mult": cpu * 1e9 / n as f64, "load1": loadavg()[0]}));
    }
    out["field_mult_gf127"] = json!(f);

    // 2. Exact row recompute (the localization of a failed Freivalds row): k x n checked i128 multiply-accumulates.
    let (k, nn) = (
        arg(args, "--k").unwrap_or_else(|| "896".into()).parse::<usize>().unwrap_or(896),
        arg(args, "--n").unwrap_or_else(|| "151936".into()).parse::<usize>().unwrap_or(151936),
    );
    let x: Vec<i128> = (0..k).map(|_| (xorshift(&mut st) % 255) as i128 - 127).collect();
    let w: Vec<i128> = (0..k * nn).map(|_| (xorshift(&mut st) % 255) as i128 - 127).collect();
    let mut e = vec![];
    for _ in 0..reps {
        let (s, wall, cpu) = timed(|| {
            let mut acc = 0i128;
            for j in 0..nn {
                let (mut pos, mut neg) = (0i128, 0i128);
                for kk in 0..k {
                    let t = x[kk].checked_mul(w[kk * nn + j]).unwrap();
                    if t > 0 {
                        pos = pos.checked_add(t).unwrap();
                    } else {
                        neg = neg.checked_add(t).unwrap();
                    }
                }
                acc ^= pos + neg;
            }
            acc
        });
        std::hint::black_box(s);
        e.push(json!({"k": k, "n": nn, "wall_s": wall, "cpu_s": cpu, "cpu_ns_per_mac": cpu * 1e9 / (k * nn) as f64, "load1": loadavg()[0]}));
    }
    out["exact_row_recompute"] = json!(e);
    drop(w);

    // 3. Tensor commitment (the dual-root Merkle commitment every opened value and weight is authenticated by), MB/s of dtype width.
    let elems = 8_000_000usize;
    let data: Vec<i128> = (0..elems).map(|_| (xorshift(&mut st) % 255) as i128 - 127).collect();
    let t = Tensor::new(DType::I8, vec![elems / 1000, 1000], data).map_err(|e| e.to_string())?;
    let mut c = vec![];
    for _ in 0..reps {
        let (d, wall, cpu) = timed(|| tensor_commitment(&t));
        std::hint::black_box(d);
        c.push(json!({"elements": elems, "wall_s": wall, "cpu_s": cpu, "cpu_ns_per_element": cpu * 1e9 / elems as f64, "load1": loadavg()[0]}));
    }
    out["tensor_commitment_i8"] = json!(c);

    // 4. Reading a container tensor (file -> i128 tensor) and a raw sequential read of the file.
    if let Some(path) = arg(args, "--file") {
        let cont = PalwTirContainerV1::open(Path::new(&path)).map_err(|e| e.to_string())?;
        let biggest = cont.header.tensors.iter().max_by_key(|e| e.bytes).unwrap().clone();
        let mut r = vec![];
        for _ in 0..reps.min(3) {
            let (tn, wall, cpu) = timed(|| cont.read_tensor(biggest.param, biggest.layer).unwrap());
            r.push(json!({"param": biggest.param, "bytes": biggest.bytes, "elements": tn.data.len(), "wall_s": wall, "cpu_s": cpu,
                          "cpu_ns_per_element": cpu * 1e9 / tn.data.len() as f64, "load1": loadavg()[0]}));
        }
        out["read_tensor_biggest"] = json!(r);
        let mut seq = vec![];
        for _ in 0..reps.min(3) {
            let (bytes, wall, cpu) = timed(|| {
                let mut f = std::fs::File::open(&path).unwrap();
                let mut buf = vec![0u8; 8 << 20];
                let mut total = 0u64;
                loop {
                    let k = f.read(&mut buf).unwrap();
                    if k == 0 {
                        break;
                    }
                    total += k as u64;
                }
                total
            });
            seq.push(json!({"bytes": bytes, "wall_s": wall, "cpu_s": cpu, "MB_per_s": bytes as f64 / 1e6 / wall, "load1": loadavg()[0], "note": "page cache is warm after the first pass"}));
        }
        out["sequential_read"] = json!(seq);
    }
    out["load_after"] = json!(loadavg());
    out["peak_rss_bytes"] = json!(rusage().2);
    Ok(out)
}
