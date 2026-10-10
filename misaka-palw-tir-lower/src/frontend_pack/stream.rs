//! Bounded raw-byte conversion. Ordinary integer imports never take a detour through f32.
use super::*;
use misaka_palw_tir::prim::Rounding;
use std::sync::atomic::{AtomicU64, Ordering};

pub const RECORD_FORMAT: &str = "misaka.palw.tir-frontend-build.v1";
pub const MAX_BLOCK_BYTES: usize = 16 << 20;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SourceRecord {
    pub name: String,
    pub dtype: String,
    pub shape: Vec<usize>,
    pub digest: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DescriptorRecord {
    pub id: String,
    pub digest: String,
}

/// A reproducible build receipt, not a source-fidelity or conformance certificate.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BuildRecord {
    pub format: String,
    pub compiler_version: String,
    pub compiler_source_digest: String,
    pub pack_hash: String,
    pub config_hash: String,
    pub program_digest: String,
    pub tokenizer_id: String,
    pub artifact_digest: String,
    pub scope: Scope,
    pub assumed_defaults: Vec<String>,
    pub source_tensors: Vec<SourceRecord>,
    pub saturated_values: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub quant_formats: Vec<DescriptorRecord>,
}

pub struct Conversion {
    pub record: BuildRecord,
    pub tensor_bytes: u64,
    /// Unique stored input bytes, independently of decoded integer artifact size.
    pub source_bytes: u64,
    /// Largest source range actually requested, independent of checkpoint size or row width.
    pub max_read_bytes: usize,
    /// Actual requested raw source bytes, including descriptor pinning scans and cache misses.
    /// This operational measurement is excluded from reproducible identity.
    pub source_read_bytes: u64,
}

struct Temp(std::path::PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

impl Compiled {
    pub fn write(&self, path: &Path, source: &dyn TensorSource, tokenizer_id: [u8; 64], block_bytes: usize) -> Result<Conversion> {
        self.write_checked(path, source, tokenizer_id, block_bytes, None)
    }

    /// Rebuild an independently supplied receipt; a mismatch never replaces the existing output.
    pub fn write_checked(
        &self,
        path: &Path,
        source: &dyn TensorSource,
        tokenizer_id: [u8; 64],
        block_bytes: usize,
        expected_record: Option<&BuildRecord>,
    ) -> Result<Conversion> {
        if !(8..=MAX_BLOCK_BYTES).contains(&block_bytes) {
            return Err(bad("FRONTEND_STREAM_LIMIT: block bytes must be 8..=16MiB"));
        }
        if self.metadata_max_read_bytes > block_bytes {
            return Err(bad("FRONTEND_STREAM_LIMIT: recompile metadata with the requested block budget"));
        }
        source.validate_snapshot()?;
        let names: BTreeSet<_> = source.names().into_iter().collect();
        let expected: BTreeSet<_> = self.bindings.values().flat_map(|r| r.sources.keys().cloned()).collect();
        if names != expected {
            return Err(bad("FRONTEND_BINDING: source inventory changed since compilation"));
        }
        for r in self.bindings.values() {
            for (name, meta) in &r.sources {
                if source.metadata(name).as_ref() != Some(meta) {
                    return Err(bad("FRONTEND_BINDING: source header changed since compilation"));
                }
            }
        }
        let sources: BTreeMap<_, _> = self.bindings.values().flat_map(|r| r.sources.iter().map(|(n, m)| (n, m))).collect();
        let source_bytes = sources
            .values()
            .try_fold(0u64, |n, m| n.checked_add(m.bytes))
            .ok_or_else(|| bad("FRONTEND_STREAM_LIMIT: aggregate source byte count"))?;
        let parent = path.parent().unwrap_or(Path::new("."));
        let temp =
            Temp(parent.join(format!(".palw-frontend-{}-{}.tmp", std::process::id(), NEXT_TEMP.fetch_add(1, Ordering::Relaxed))));
        let reserved =
            std::fs::OpenOptions::new().write(true).create_new(true).open(&temp.0).map_err(|e| LowerError::Io(e.to_string()))?;
        drop(reserved);
        let meta = serde_json::json!({"frontend": {"kind":"tir-frontend-pack", "id":self.pack_id, "hash":self.pack_hash},
            "config_hash":self.config_hash, "scope":self.scope, "source_equivalence":"SOURCE_EQUIVALENCE_UNVERIFIED"})
        .to_string();
        let mut records = BTreeMap::<String, SourceRecord>::new();
        let mut saturated_values = 0u64;
        let mut tensor_bytes = 0u64;
        let mut max_read_bytes = self.metadata_max_read_bytes;
        let mut source_read_bytes = self.metadata_read_bytes;
        let mut quant_formats = BTreeMap::new();
        let artifact_digest = misaka_palw_tir_artifact::write_container_v1_streamed(
            &temp.0,
            &self.program,
            Vec::new(),
            tokenizer_id,
            meta,
            &mut |j, l, out| {
                let r = &self.bindings[&(j, l)];
                let dtype = self.program.params[j as usize].dtype;
                if let Some(decoder) = &r.decoder {
                    let converted =
                        decoder.write(&r.sources, source, dtype, &r.binding.import, out, block_bytes).map_err(|e| e.to_string())?;
                    max_read_bytes = max_read_bytes.max(converted.peak);
                    source_read_bytes += converted.read_bytes;
                    tensor_bytes += converted.bytes;
                    saturated_values += converted.saturated;
                    if let Import::Descriptor { format, .. } = &r.binding.import {
                        quant_formats.insert(format.clone(), decoder.format.digest_hex());
                    }
                    for record in converted.records {
                        if records.get(&record.name).is_some_and(|previous| previous != &record) {
                            return Err("FRONTEND_BINDING: tied descriptor tensor changed during conversion".into());
                        }
                        records.insert(record.name.clone(), record);
                    }
                    return Ok(());
                }
                let width = stored_width(&r.meta.dtype).expect("preflight storage type");
                // Bound both the stored and encoded buffers, even for I8 -> I64 widening.
                let block = (block_bytes / width.max(dtype.width()) * width) as u64;
                let mut hash = blake2b_simd::Params::new().hash_length(64).key(b"MISAKA/PALW/TIR/FRONTEND/TENSOR/V1").to_state();
                hash.update(canonical_json(&serde_json::json!({"dtype":r.meta.dtype,"shape":r.meta.shape})).as_bytes());
                let mut at = 0u64;
                while at < r.meta.bytes {
                    let end = at.saturating_add(block).min(r.meta.bytes);
                    let bytes = source.read_slice(&r.binding.source, at..end).map_err(|e| e.to_string())?;
                    if bytes.len() as u64 != end - at {
                        return Err(format!("FRONTEND_BINDING: short range of {}", r.binding.source));
                    }
                    max_read_bytes = max_read_bytes.max(bytes.len());
                    source_read_bytes += bytes.len() as u64;
                    hash.update(&bytes);
                    let mut encoded = Vec::with_capacity(bytes.len() / width * dtype.width());
                    for chunk in bytes.chunks_exact(width) {
                        let value = match r.binding.import {
                            Import::Integer => integer(&r.meta.dtype, chunk),
                            Import::FixedPoint { shift, round, .. } => {
                                ieee_integer(&r.meta.dtype, chunk, shift, round.compile()).map_err(|e| e.to_string())?
                            }
                            Import::Descriptor { .. } => unreachable!("handled by bounded descriptor writer"),
                        };
                        let value = if dtype.contains(value) {
                            value
                        } else {
                            match r.binding.import {
                                Import::FixedPoint { overflow: Overflow::Saturate, .. } => {
                                    saturated_values += 1;
                                    value.clamp(dtype.min_value(), dtype.max_value())
                                }
                                _ => return Err(format!("FRONTEND_QUANT_RANGE: {} value outside {}", r.binding.source, dtype.name())),
                            }
                        };
                        dtype.encode_le(value, &mut encoded);
                    }
                    tensor_bytes += encoded.len() as u64;
                    out.write_all(&encoded).map_err(|e| e.to_string())?;
                    at = end;
                }
                let record = SourceRecord {
                    name: r.binding.source.clone(),
                    dtype: r.meta.dtype.clone(),
                    shape: r.meta.shape.clone(),
                    digest: program::hex(hash.finalize().as_bytes()),
                };
                if records.get(&record.name).is_some_and(|previous| previous != &record) {
                    return Err("FRONTEND_BINDING: tied tensor changed during conversion".into());
                }
                records.insert(record.name.clone(), record);
                Ok(())
            },
        )
        .map_err(|e| LowerError::weights(e.to_string()))?;
        let record = BuildRecord {
            format: RECORD_FORMAT.into(),
            compiler_version: env!("CARGO_PKG_VERSION").into(),
            compiler_source_digest: compiler_digest(),
            pack_hash: self.pack_hash.clone(),
            config_hash: self.config_hash.clone(),
            program_digest: crate::artifact::program_digest(&self.program),
            tokenizer_id: program::hex(&tokenizer_id),
            artifact_digest: program::hex(&artifact_digest),
            scope: self.scope.clone(),
            assumed_defaults: self.assumed_defaults.clone(),
            source_tensors: records.into_values().collect(),
            saturated_values,
            quant_formats: quant_formats.into_iter().map(|(id, digest)| DescriptorRecord { id, digest }).collect(),
        };
        if expected_record.is_some_and(|expected| expected != &record) {
            return Err(bad("FRONTEND_BUILD_MISMATCH: source, frontend, tokenizer or artifact differs"));
        }
        // A failure above leaves any previous artifact intact and removes the incomplete temporary.
        source.validate_snapshot()?;
        std::fs::rename(&temp.0, path).map_err(|e| LowerError::Io(e.to_string()))?;
        Ok(Conversion { tensor_bytes, source_bytes, max_read_bytes, source_read_bytes, record })
    }
}

fn integer(dtype: &str, bytes: &[u8]) -> i128 {
    let mut raw = [0u8; 8];
    raw[..bytes.len()].copy_from_slice(bytes);
    let u = u64::from_le_bytes(raw);
    match dtype {
        "I8" => u as i8 as i128,
        "I16" => u as i16 as i128,
        "I32" => u as i32 as i128,
        "I64" => u as i64 as i128,
        _ => u as i128,
    }
}

/// Decode IEEE bits to sign × mantissa × 2^exponent; scale and round in integers.
/// Huge finite results are represented by a signed i128 sentinel, well outside every param dtype.
pub(super) fn ieee_integer(dtype: &str, bytes: &[u8], shift: i16, rule: Rounding) -> Result<i128> {
    let (fraction_bits, exponent_bits, bias) = match dtype {
        "BF16" => (7, 8, 127),
        "F16" => (10, 5, 15),
        "F32" => (23, 8, 127),
        "F64" => (52, 11, 1023),
        _ => return Err(bad("SOURCE_FORMAT_UNSUPPORTED")),
    };
    let mut raw = [0u8; 8];
    raw[..bytes.len()].copy_from_slice(bytes);
    let bits = u64::from_le_bytes(raw);
    let negative = bits >> (fraction_bits + exponent_bits) != 0;
    let exp = ((bits >> fraction_bits) & ((1 << exponent_bits) - 1)) as i32;
    let frac = bits & ((1u64 << fraction_bits) - 1);
    if exp == (1 << exponent_bits) - 1 {
        return Err(bad("FRONTEND_QUANT_NONFINITE: NaN or infinity"));
    }
    let mantissa = (if exp == 0 { frac } else { frac | (1u64 << fraction_bits) }) as u128;
    if mantissa == 0 {
        return Ok(0);
    }
    let exponent = exp.max(1) - bias - fraction_bits + shift as i32;
    let magnitude = if exponent >= 0 {
        mantissa
            .checked_shl(exponent as u32)
            .filter(|n| (*n >> exponent.min(127)) == mantissa && *n <= i128::MAX as u128)
            .unwrap_or(i128::MAX as u128)
    } else {
        let n = (-exponent) as u32;
        let (q, r, half) =
            if n >= 128 { (0, mantissa, u128::MAX) } else { (mantissa >> n, mantissa & ((1u128 << n) - 1), 1u128 << (n - 1)) };
        let up = match rule {
            Rounding::Floor => negative && r != 0,
            Rounding::HalfUp => {
                if negative {
                    r > half
                } else {
                    r >= half
                }
            }
            Rounding::HalfAwayFromZero => r >= half,
        };
        q + u128::from(up)
    };
    Ok(if negative { -(magnitude as i128) } else { magnitude as i128 })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ieee_quantization_preserves_ties_subnormals_and_wide_precision() {
        let q = |x: f64, r| ieee_integer("F64", &x.to_le_bytes(), 0, r).unwrap();
        assert_eq!(q(-2.5, Rounding::HalfUp), -2);
        assert_eq!(q(-2.5, Rounding::HalfAwayFromZero), -3);
        assert_eq!(q(2.5, Rounding::Floor), 2);
        assert_eq!(q(-f64::from_bits(1), Rounding::Floor), -1);
        assert_eq!(q(f64::from_bits(1), Rounding::HalfUp), 0);
        assert_eq!(q(9_007_199_254_740_994.0, Rounding::HalfUp), 9_007_199_254_740_994);
        assert_eq!(q(f64::MAX, Rounding::Floor), i128::MAX);
        assert_eq!(q(-f64::MAX, Rounding::Floor), -i128::MAX);
        assert!(ieee_integer("F32", &f32::NAN.to_le_bytes(), 0, Rounding::Floor).is_err());
        assert!(ieee_integer("F16", &0x7c00u16.to_le_bytes(), 0, Rounding::Floor).is_err());
        assert_eq!(ieee_integer("BF16", &0x3f80u16.to_le_bytes(), 12, Rounding::Floor).unwrap(), 4096);
        assert_eq!(ieee_integer("F16", &0xc100u16.to_le_bytes(), 0, Rounding::HalfAwayFromZero).unwrap(), -3);
    }
}
