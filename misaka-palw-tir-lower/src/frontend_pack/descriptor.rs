//! Bounded composition of public, data-only packed tensor descriptors.
use super::*;
use crate::quantfmt::{
    QuantFormat,
    desc::{LayoutDesc, QuantFormatDesc},
    tensors::RoleTensor,
    virt::VirtualFormat,
};
use std::cell::RefCell;
use std::io::Write;
use std::ops::Range;
use std::sync::Arc;

pub(super) const MAX_FORMATS: usize = 16;

// A descriptor may consume one nested parameter/check path without silently consuming its siblings.
fn config_paths(d: &QuantFormatDesc) -> Result<Vec<String>> {
    let mut paths = vec!["quant_method".to_string()];
    paths.extend(d.params.values().filter_map(|p| p.config.clone()));
    if let Some(c) = &d.config {
        paths.extend(c.inert.iter().cloned());
        paths.extend(c.skip.iter().cloned());
        paths.extend(c.checks.iter().map(|c| c.path.clone()));
    }
    for p in &paths {
        if p.len() > 128
            || p.split('.').count() > 16
            || p.split('.').any(|s| {
                let (key, index) = s.split_once('[').map_or((s, None), |(k, i)| (k, Some(i)));
                key.is_empty()
                    || key.contains(']')
                    || index.is_some_and(|i| !i.ends_with(']') || i[..i.len() - 1].parse::<usize>().is_err())
            })
        {
            return Err(bad("FRONTEND_DESCRIPTOR: invalid config path"));
        }
    }
    Ok(paths)
}
fn check_config(d: &QuantFormatDesc, config: &Value) -> Result<()> {
    fn visit(v: &Value, at: &str, paths: &[String]) -> Result<()> {
        if paths.iter().any(|p| p == at) {
            return Ok(());
        }
        if !at.is_empty() && !paths.iter().any(|p| p.starts_with(&format!("{at}.")) || p.starts_with(&format!("{at}["))) {
            return Err(bad(format!("FRONTEND_DESCRIPTOR: config path {at} does not read any declared parameter/check")));
        }
        match v {
            Value::Object(o) => {
                for (k, child) in o {
                    if k.contains(['.', '[', ']']) {
                        return Err(bad("FRONTEND_DESCRIPTOR: ambiguous config key"));
                    }
                    let path = if at.is_empty() { k.clone() } else { format!("{at}.{k}") };
                    visit(child, &path, paths)?;
                }
            }
            Value::Array(a) => {
                for (i, child) in a.iter().enumerate() {
                    visit(child, &format!("{at}[{i}]"), paths)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    visit(config, "", &config_paths(d)?)
}

/// Compile and validate every declared vector before any checkpoint weight is read. A descriptor's
/// own vectors establish consistency with its pinned expected bytes, never source-model fidelity.
pub(super) fn parse(value: &Value, test_work: &mut usize) -> Result<Arc<QuantFormat>> {
    let text = value.to_string();
    if text.len() > 256 << 10 {
        return Err(bad("FRONTEND_DESCRIPTOR_LIMIT: descriptor bytes"));
    }
    let d = QuantFormatDesc::from_json(&text).map_err(|e| bad(e.to_string()))?;
    let LayoutDesc::Virtual { roles, shape, checks, .. } = &d.layout else {
        return Err(bad("FRONTEND_DESCRIPTOR_EXTENSION_REQUIRED: bounded frontend import requires a virtual tensor layout"));
    };
    if roles.len() > 16
        || d.params.len() > 32
        || d.tables.len() > 16
        || checks.len() > 32
        || d.tests.is_empty()
        || d.tests.len() > 32
        || d.params.values().any(|p| p.from_role.is_some())
    {
        return Err(bad("FRONTEND_DESCRIPTOR_LIMIT: roles/parameters/tables/checks/vectors"));
    }
    // Bound parsing before compiling: left-associative chains and nested calls are both finite.
    for s in shape.iter().chain(checks.iter().map(|c| &c.expr)).chain(d.decode.value.iter()) {
        crate::quantfmt::expr::streaming_syntax(s).map_err(|e| bad(e.to_string()))?;
    }
    config_paths(&d)?;
    let table_bytes: usize = d.tables.values().map(|t| t.hex.len() / 2).sum();
    if table_bytes > 64 << 10 {
        return Err(bad("FRONTEND_DESCRIPTOR_LIMIT: table bytes"));
    }
    let v = VirtualFormat::compile(&d).map_err(|e| bad(e.to_string()))?;
    let work = v.streaming_work().map_err(|e| bad(e.to_string()))?;
    for test in &d.tests {
        if !test.block_hex.is_empty() {
            return Err(bad("FRONTEND_DESCRIPTOR: block bytes in virtual vector"));
        }
        if test.roles.keys().any(|r| !roles.iter().any(|x| &x.name == r)) {
            return Err(bad("FRONTEND_DESCRIPTOR: undeclared vector role"));
        }
        let headers: Vec<_> =
            roles.iter().map(|r| test.roles.get(&r.name).map(|t| RoleTensor::header_only(t.shape.clone(), &t.dtype))).collect();
        for t in test.roles.values() {
            let bytes = RoleTensor::header_only(t.shape.clone(), &t.dtype)
                .stored_bytes()
                .ok_or_else(|| bad("FRONTEND_DESCRIPTOR_LIMIT: vector shape/dtype"))?;
            if bytes > 64 << 10 || bytes.checked_mul(2) != Some(t.hex.len()) || t.shape.iter().any(|n| *n > i64::MAX as usize) {
                return Err(bad("FRONTEND_DESCRIPTOR_LIMIT: vector bytes/shape"));
            }
        }
        let config = test.config.clone().unwrap_or_else(|| serde_json::json!({}));
        check_config(&d, &config)?;
        let params = v.read_config(&config).map_err(|e| bad(e.to_string()))?.params;
        let shape = v.shape(&headers, &params).map_err(|e| bad(e.to_string()))?;
        let count = shape
            .iter()
            .try_fold(1usize, |n, d| n.checked_mul(*d))
            .ok_or_else(|| bad("FRONTEND_DESCRIPTOR_LIMIT: vector shape overflow"))?;
        if count > 8192 || count.checked_mul(8) != Some(test.values_f32_hex.len()) {
            return Err(bad("FRONTEND_DESCRIPTOR_LIMIT: vector output bytes"));
        }
        *test_work = test_work.checked_add(count * work).ok_or_else(|| bad("FRONTEND_DESCRIPTOR_LIMIT: vector work"))?;
        if *test_work > 4_000_000 {
            return Err(bad("FRONTEND_DESCRIPTOR_LIMIT: vector work"));
        }
    }
    Ok(Arc::new(QuantFormat::from_json(&text)?))
}

#[derive(Clone, Debug)]
pub(super) struct Decoder {
    pub format: Arc<QuantFormat>,
    headers: Vec<Option<RoleTensor>>,
    names: Vec<Option<String>>,
    params: BTreeMap<String, i64>,
}

pub(super) fn resolve(
    format: Arc<QuantFormat>,
    roles: &BTreeMap<String, String>,
    config: &Value,
    source: &dyn TensorSource,
) -> Result<(Decoder, TensorMeta, BTreeMap<String, TensorMeta>)> {
    let v = format.as_virtual().expect("bounded parser accepts virtual layouts only");
    if roles.keys().any(|r| !v.roles().any(|(n, _, _)| n == r)) {
        return Err(bad("FRONTEND_BINDING: undeclared descriptor role"));
    }
    let mut headers = Vec::new();
    let mut names = Vec::new();
    let mut sources = BTreeMap::new();
    for (r, _, required) in v.roles() {
        match roles.get(r) {
            Some(name) => {
                let meta = source.metadata(name).ok_or_else(|| bad(format!("FRONTEND_BINDING: missing tensor {name}")))?;
                let header = RoleTensor::header_only(meta.shape.clone(), &meta.dtype);
                if meta.shape.iter().any(|n| *n == 0 || *n > i64::MAX as usize)
                    || header.stored_bytes().and_then(|n| u64::try_from(n).ok()) != Some(meta.bytes)
                {
                    return Err(bad("FRONTEND_BINDING: descriptor source byte count/shape"));
                }
                headers.push(Some(header));
                names.push(Some(name.clone()));
                sources.insert(name.clone(), meta);
            }
            None if required => return Err(bad(format!("FRONTEND_BINDING: missing descriptor role {r}"))),
            None => {
                headers.push(None);
                names.push(None);
            }
        }
    }
    check_config(&format.desc, config)?;
    let params = v.read_config(config).map_err(|e| bad(e.to_string()))?.params;
    let shape = v.shape(&headers, &params).map_err(|e| bad(e.to_string()))?;
    let bytes =
        shape.iter().try_fold(4usize, |n, d| n.checked_mul(*d)).ok_or_else(|| bad("FRONTEND_BINDING: decoded shape overflow"))?;
    let meta = TensorMeta { dtype: "F32".into(), shape, bytes: bytes as u64 };
    Ok((Decoder { format, headers, names, params }, meta, sources))
}

/// One bounded page per role, with total retained raw pages <= block_bytes. This intentionally
/// preserves arbitrary strided accesses: it never densifies experts or loads a whole packed role.
struct Pages<'a> {
    source: &'a dyn TensorSource,
    names: &'a [Option<String>],
    headers: &'a [Option<RoleTensor>],
    bytes: usize,
    pages: RefCell<Vec<Option<(Range<usize>, Vec<u8>)>>>,
    peak: std::cell::Cell<usize>,
    read_bytes: std::cell::Cell<u64>,
}
impl Pages<'_> {
    fn read(&self, role: usize, range: Range<usize>) -> crate::quantfmt::expr::R<Vec<u8>> {
        use crate::quantfmt::expr::DslError;
        let header = self.headers[role].as_ref().ok_or_else(|| DslError("absent role".into()))?;
        let total = header.stored_bytes().ok_or_else(|| DslError("role shape overflow".into()))?;
        if range.start > range.end || range.end > total {
            return Err(DslError("role byte range".into()));
        }
        let mut pages = self.pages.borrow_mut();
        if !pages[role].as_ref().is_some_and(|(r, _)| r.start <= range.start && range.end <= r.end) {
            let width = total / header.shape.iter().product::<usize>();
            let page = (self.bytes / width).max(1) * width;
            let start = range.start / page * page;
            let end = start.saturating_add(page).min(total);
            pages[role] = None;
            let data = self
                .source
                .read_slice(self.names[role].as_ref().expect("present header"), start as u64..end as u64)
                .map_err(|e| DslError(e.to_string()))?;
            if data.len() != end - start {
                return Err(DslError("FRONTEND_BINDING: short descriptor range".into()));
            }
            self.peak.set(self.peak.get().max(data.len()));
            self.read_bytes.set(self.read_bytes.get().saturating_add(data.len() as u64));
            pages[role] = Some((start..end, data));
        }
        let (r, data) = pages[role].as_ref().expect("loaded page");
        Ok(data[range.start - r.start..range.end - r.start].to_vec())
    }
}

pub(super) struct Written {
    pub records: Vec<SourceRecord>,
    pub saturated: u64,
    pub bytes: u64,
    pub peak: usize,
    pub read_bytes: u64,
}
fn hash_sources(
    sources: &BTreeMap<String, TensorMeta>,
    source: &dyn TensorSource,
    block: usize,
) -> Result<(Vec<SourceRecord>, usize, u64)> {
    let mut records = Vec::new();
    let mut peak = 0;
    let mut traffic = 0;
    for (name, meta) in sources {
        let mut h = blake2b_simd::Params::new().hash_length(64).key(b"MISAKA/PALW/TIR/FRONTEND/TENSOR/V1").to_state();
        h.update(canonical_json(&serde_json::json!({"dtype":meta.dtype,"shape":meta.shape})).as_bytes());
        let mut at = 0;
        while at < meta.bytes {
            let end = at.saturating_add(block as u64).min(meta.bytes);
            let raw = source.read_slice(name, at..end)?;
            if raw.len() as u64 != end - at {
                return Err(bad("FRONTEND_BINDING: short descriptor hash range"));
            }
            h.update(&raw);
            peak = peak.max(raw.len());
            traffic += raw.len() as u64;
            at = end;
        }
        records.push(SourceRecord {
            name: name.clone(),
            dtype: meta.dtype.clone(),
            shape: meta.shape.clone(),
            digest: program::hex(h.finalize().as_bytes()),
        });
    }
    Ok((records, peak, traffic))
}
impl Decoder {
    pub(super) fn write(
        &self,
        sources: &BTreeMap<String, TensorMeta>,
        source: &dyn TensorSource,
        dtype: misaka_palw_tir::DType,
        import: &Import,
        out: &mut dyn Write,
        block: usize,
    ) -> Result<Written> {
        let Import::Descriptor { shift, round, overflow, .. } = import else { unreachable!() };
        let present = self.headers.iter().filter(|h| h.is_some()).count();
        if block < present * 8 {
            return Err(bad("FRONTEND_STREAM_LIMIT: block budget must hold one element per descriptor role"));
        }
        let (records, mut peak, mut read_bytes) = hash_sources(sources, source, block)?;
        let pages = Pages {
            source,
            names: &self.names,
            headers: &self.headers,
            bytes: block / present,
            pages: RefCell::new(vec![None; self.headers.len()]),
            peak: std::cell::Cell::new(0),
            read_bytes: std::cell::Cell::new(0),
        };
        let v = self.format.as_virtual().expect("compiled virtual");
        let total = v.shape(&self.headers, &self.params).map_err(|e| bad(e.to_string()))?.iter().product::<usize>();
        let chunk = (block / dtype.width().max(4)).min(1024);
        let mut saturated = 0;
        let mut bytes = 0;
        for at in (0..total).step_by(chunk) {
            let values = v
                .decode_range_streamed(&self.headers, &self.params, at..(at + chunk).min(total), &|r, range| pages.read(r, range))
                .map_err(|e| bad(e.to_string()))?;
            let mut encoded = Vec::with_capacity(values.len() * dtype.width());
            for f in values {
                let value = super::stream::ieee_integer("F32", &f.to_le_bytes(), *shift, round.compile())?;
                let value = if dtype.contains(value) {
                    value
                } else {
                    match overflow {
                        Overflow::Saturate => {
                            saturated += 1;
                            value.clamp(dtype.min_value(), dtype.max_value())
                        }
                        Overflow::Reject => return Err(bad("FRONTEND_QUANT_RANGE: descriptor output outside target integer")),
                    }
                };
                dtype.encode_le(value, &mut encoded);
            }
            out.write_all(&encoded).map_err(|e| LowerError::Io(e.to_string()))?;
            bytes += encoded.len() as u64;
        }
        peak = peak.max(pages.peak.get());
        read_bytes += pages.read_bytes.get();
        // Release caches before hashing; data changed during conversion must not enter a receipt.
        drop(pages);
        let (after, after_peak, after_bytes) = hash_sources(sources, source, block)?;
        if records != after {
            return Err(bad("FRONTEND_BINDING: descriptor source changed during conversion"));
        }
        Ok(Written { records, saturated, bytes, peak: peak.max(after_peak), read_bytes: read_bytes + after_bytes })
    }
}
