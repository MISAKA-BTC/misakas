//! Isolated QF1 source embedding check. No claim or eligibility state consumes its output.
use misaka_palw_tir::Rounding;
use misaka_palw_tir_exec::scalar::shr_round_i128;
use misaka_palw_tir_lower::{
    quant::{mul_shift, quantize_rows16},
    weights::widen_floats,
};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, io::Write, path::Path};

const WIDTH: usize = 4096;
const SOURCE_ROW: usize = WIDTH * 2;
// Extracted from the complete hash-verified upstream tensor by check_embedding.py.
const SOURCE_ROWS_SHA256: &str = "04dbb6c1ca08772b35c89e1ab308699c24a30b0fc50e900787b682d3aa02a676";
// Frozen conversion for this isolated probe, common to all positions and seeds.
const RESIDUAL_SCALE: f64 = 1.0 / 32768.0;

type SourceRows<'a> = BTreeMap<u32, (&'a [u8], Vec<i16>)>;

fn prepare(data: &[u8]) -> Result<SourceRows<'_>, &'static str> {
    if data.len() != 48 * (4 + SOURCE_ROW) || faster_hex::hex_string(&Sha256::digest(data)) != SOURCE_ROWS_SHA256 {
        return Err("source embedding bundle mismatch");
    }
    let mut rows = BTreeMap::new();
    for entry in data.chunks_exact(4 + SOURCE_ROW) {
        let id = u32::from_le_bytes(entry[..4].try_into().unwrap());
        let raw = &entry[4..];
        let weights = widen_floats("BF16", raw).map_err(|_| "invalid source embedding")?;
        let converted = quantize_rows16(&weights, 1, WIDTH);
        let (m, s) = mul_shift(converted.scales[0] / RESIDUAL_SCALE);
        let mut lifted = Vec::with_capacity(WIDTH);
        for code in converted.codes {
            let rounded = shr_round_i128(i128::from(code) * i128::from(m), s as u32, Rounding::HalfAwayFromZero);
            lifted.push(rounded.clamp(-32767, 32767) as i16);
        }
        rows.insert(id, (raw, lifted));
    }
    Ok(rows)
}

fn lookup(rows: &SourceRows<'_>, tokens: &[u32]) -> Result<(String, String), &'static str> {
    let mut source = Sha256::new();
    let mut residual = Sha256::new();
    for token in tokens {
        let (raw, lifted) = rows.get(token).ok_or("token absent from private source rows")?;
        source.update(raw);
        for code in lifted {
            residual.update(code.to_le_bytes());
        }
    }
    Ok((faster_hex::hex_string(&source.finalize()), faster_hex::hex_string(&residual.finalize())))
}

pub(super) fn check_input(path: &Path, tokens: &[u32]) {
    let mut data = fs::read(path).unwrap();
    let rows = prepare(&data).unwrap();
    let (raw, residual) = lookup(&rows, tokens).unwrap();
    if let Some(path) = std::env::var_os("MISAKA_PRIVATE_QF1_ATTENTION") {
        attention::check(Path::new(&path), &rows, tokens);
    }
    assert_eq!(lookup(&rows, &[151936]), Err("token absent from private source rows"));
    data[4] ^= 1;
    assert_eq!(prepare(&data).unwrap_err(), "source embedding bundle mismatch");
    let line =
        format!("QF1 embedding pid={} rows={} bf16_sha256={raw} residual_i16_sha256={residual}\n", std::process::id(), tokens.len());
    std::io::stderr().lock().write_all(line.as_bytes()).unwrap();
}

#[path = "tb1_qf1_attention_fixture.rs"]
mod attention;
