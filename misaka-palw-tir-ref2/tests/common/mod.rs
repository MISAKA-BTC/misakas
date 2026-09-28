//! Shared helpers of the integration tests: golden-vector JSON (04b §12) into this crate's types.
#![allow(dead_code)]

use std::path::PathBuf;

use misaka_palw_tir_ref2::{DType, Tensor};
use serde_json::Value;

pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

pub fn vectors_dir() -> PathBuf {
    repo_root().join("consensus-vectors").join("tir-v1")
}

pub fn read_json(path: &std::path::Path) -> Value {
    let s = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&s).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

pub fn hex_decode(s: &str) -> Vec<u8> {
    assert!(s.len() % 2 == 0, "odd hex length");
    let nib = |c: u8| -> u8 {
        match c {
            b'0'..=b'9' => c - b'0',
            b'a'..=b'f' => c - b'a' + 10,
            b'A'..=b'F' => c - b'A' + 10,
            _ => panic!("bad hex digit {c}"),
        }
    };
    s.as_bytes().chunks(2).map(|p| nib(p[0]) << 4 | nib(p[1])).collect()
}

pub fn hex_encode(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// §12: every integer is a decimal string (numbers are also accepted for shapes).
pub fn int_of(v: &Value) -> i128 {
    match v {
        Value::String(s) => s.parse::<i128>().unwrap_or_else(|e| panic!("integer {s}: {e}")),
        Value::Number(n) => n.as_i64().map(|x| x as i128).or_else(|| n.as_u64().map(|x| x as i128)).expect("integer"),
        _ => panic!("not an integer: {v}"),
    }
}

pub fn dtype_of(v: &Value) -> DType {
    DType::from_name(v.as_str().expect("dtype string")).expect("known dtype")
}

pub fn shape_of(v: &Value) -> Vec<u64> {
    v.as_array().expect("shape array").iter().map(|d| int_of(d) as u64).collect()
}

/// A tensor `{dtype, shape, data}`.
pub fn tensor_of(v: &Value) -> Tensor {
    let dtype = dtype_of(&v["dtype"]);
    let shape = shape_of(&v["shape"]);
    let data = v["data"].as_array().expect("data array").iter().map(int_of).collect();
    Tensor::new(dtype, shape, data).expect("well-formed golden tensor")
}

pub fn tensor_json(t: &Tensor) -> String {
    let data: Vec<String> = t.data.iter().map(|v| format!("\"{v}\"")).collect();
    format!("{{\"dtype\":\"{}\",\"shape\":{:?},\"data\":[{}]}}", t.dtype.name(), t.shape, data.join(","))
}
