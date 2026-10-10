//! Bounded composition of public, data-only packed tensor descriptors.
use super::*;
use crate::quantfmt::{
    QuantFormat,
    desc::{LayoutDesc, QuantFormatDesc},
    tensors::RoleTensor,
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
    validate_paths(&paths)?;
    Ok(paths)
}
fn validate_paths(paths: &[String]) -> Result<()> {
    for p in paths {
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
    Ok(())
}
fn check_config(d: &QuantFormatDesc, config: &Value) -> Result<()> {
    check_paths(config, &config_paths(d)?)
}
fn check_paths(config: &Value, paths: &[String]) -> Result<()> {
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
    visit(config, "", paths)
}

/// Compile and validate every declared vector before checkpoint access. Pinned vectors establish
/// descriptor consistency, never source-model fidelity.
pub(super) fn parse(value: &Value, test_work: &mut usize) -> Result<Arc<QuantFormat>> {
    use crate::quantfmt::desc::{SizeDesc, ValDesc, unhex};
    let text = value.to_string();
    if text.len() > 256 << 10 {
        return Err(bad("FRONTEND_DESCRIPTOR_LIMIT: descriptor bytes"));
    }
    let d = QuantFormatDesc::from_json(&text).map_err(|e| bad(e.to_string()))?;
    let (roles, checks, mut syntax): (_, _, Vec<&str>) = match &d.layout {
        LayoutDesc::Virtual { roles, shape, checks, .. } => {
            (roles.as_slice(), checks.as_slice(), shape.iter().map(String::as_str).collect())
        }
        LayoutDesc::Tensors { roles, dims, checks } => {
            (roles.as_slice(), checks.as_slice(), vec![dims.out.as_str(), dims.inp.as_str()])
        }
        LayoutDesc::Blocks { fields, .. } => {
            if fields.len() > 32 || d.config.is_some() || !d.params.is_empty() {
                return Err(bad("FRONTEND_DESCRIPTOR_LIMIT: block fields or unsupported block configuration"));
            }
            (&[][..], &[][..], Vec::new())
        }
    };
    if roles.len() > 16 || d.params.len() > 32 || d.tables.len() > 16 || checks.len() > 32 || d.tests.is_empty() || d.tests.len() > 32
    {
        return Err(bad("FRONTEND_DESCRIPTOR_LIMIT: roles/parameters/tables/checks/vectors"));
    }
    syntax.extend(checks.iter().map(|c| c.expr.as_str()));
    for expression in
        [&d.decode.q, &d.decode.scale, &d.decode.zero, &d.decode.min, &d.decode.value, &d.decode.offset_term, &d.decode.order]
    {
        syntax.extend(expression.iter().map(String::as_str));
    }
    if let Some(g) = &d.decode.group {
        if let SizeDesc::Expr(s) = &g.size {
            syntax.push(s);
        }
        syntax.extend(g.index.iter().map(String::as_str));
    }
    if let Some(c) = &d.decode.code {
        for v in [&c.min, &c.max] {
            if let ValDesc::Expr(s) = v {
                syntax.push(s);
            }
        }
    }
    for s in syntax {
        crate::quantfmt::expr::streaming_syntax(s).map_err(|e| bad(e.to_string()))?;
    }
    config_paths(&d)?;
    for p in d.params.values().filter_map(|p| p.from_role.as_ref()) {
        validate_paths(&[p.path.clone()])?;
    }
    let table_bytes: usize = d.tables.values().map(|t| t.hex.len() / 2).sum();
    if table_bytes > 64 << 10 {
        return Err(bad("FRONTEND_DESCRIPTOR_LIMIT: table bytes"));
    }
    let f = QuantFormat::compile_desc(d)?;
    let work = if let Some(v) = f.as_virtual() {
        v.streaming_work()
    } else if let Some(t) = f.as_tensors() {
        t.streaming_work()
    } else {
        f.as_blocks().expect("compiled layout").streaming_work()
    }
    .map_err(|e| bad(e.to_string()))?;
    for test in &f.desc.tests {
        for r in test.roles.values() {
            let bytes = RoleTensor::header_only(r.shape.clone(), &r.dtype)
                .stored_bytes()
                .ok_or_else(|| bad("FRONTEND_DESCRIPTOR_LIMIT: vector shape/dtype"))?;
            if bytes > 64 << 10
                || bytes.checked_mul(2) != Some(r.hex.len())
                || r.shape.iter().any(|n| *n == 0 || *n > i64::MAX as usize)
            {
                return Err(bad("FRONTEND_DESCRIPTOR_LIMIT: vector bytes/shape"));
            }
        }
        let count = if let Some(b) = f.as_blocks() {
            if !test.roles.is_empty()
                || test.block_hex.len() > 2 * (64 << 10)
                || test.block_hex.is_empty()
                || test.block_hex.len() % (2 * b.bytes) != 0
                || test.config.as_ref().is_some_and(|c| c.as_object().is_none_or(|o| !o.is_empty()))
            {
                return Err(bad("FRONTEND_DESCRIPTOR_LIMIT: block vector bytes/config/roles"));
            }
            let count = test.block_hex.len() / (2 * b.bytes) * b.elems;
            b.prepare_streamed(1, count).map_err(|e| bad(e.to_string()))?;
            count
        } else {
            if !test.block_hex.is_empty() {
                return Err(bad("FRONTEND_DESCRIPTOR: block bytes in role vector"));
            }
            let declared: Vec<_> =
                if let Some(t) = f.as_tensors() { t.roles().collect() } else { f.as_virtual().unwrap().roles().collect() };
            if test.roles.keys().any(|r| !declared.iter().any(|(name, _, _)| name == r)) {
                return Err(bad("FRONTEND_DESCRIPTOR: undeclared vector role"));
            }
            let roles: Vec<_> = declared
                .iter()
                .map(|(name, _, _)| {
                    test.roles
                        .get(*name)
                        .map(|t| {
                            Ok(RoleTensor {
                                shape: t.shape.clone(),
                                dtype: t.dtype.clone(),
                                data: unhex(&t.hex).map_err(|e| bad(e.to_string()))?,
                            })
                        })
                        .transpose()
                })
                .collect::<Result<_>>()?;
            let cfg = test.config.clone().unwrap_or_else(|| serde_json::json!({}));
            check_config(&f.desc, &cfg)?;
            let params = f.read_config(&cfg)?.params;
            let shape = if let Some(t) = f.as_tensors() {
                // Validate JSON nesting before the exact-number metadata parser is entered.
                for p in f.desc.params.values().filter_map(|p| p.from_role.as_ref()) {
                    let slot = declared.iter().position(|(name, _, _)| *name == p.role).expect("compiled JSON role");
                    if let Some(role) = &roles[slot] {
                        checked_json(&role.data)?;
                    }
                }
                t.prepare_streamed(&roles, &params).map_err(|e| bad(e.to_string()))?.shape()
            } else {
                f.as_virtual().unwrap().shape(&roles, &params).map_err(|e| bad(e.to_string()))?
            };
            shape
                .iter()
                .try_fold(1usize, |n, d| n.checked_mul(*d))
                .ok_or_else(|| bad("FRONTEND_DESCRIPTOR_LIMIT: vector shape overflow"))?
        };
        if count > 8192 || count.checked_mul(8) != Some(test.values_f32_hex.len()) {
            return Err(bad("FRONTEND_DESCRIPTOR_LIMIT: vector output bytes"));
        }
        *test_work = test_work.checked_add(count * work).ok_or_else(|| bad("FRONTEND_DESCRIPTOR_LIMIT: vector work"))?;
        if *test_work > 4_000_000 {
            return Err(bad("FRONTEND_DESCRIPTOR_LIMIT: vector work"));
        }
    }
    f.run_tests()?;
    Ok(Arc::new(f))
}

fn checked_json(raw: &[u8]) -> Result<Value> {
    let json: Value = serde_json::from_slice(raw).map_err(|e| bad(format!("FRONTEND_DESCRIPTOR: role JSON: {e}")))?;
    input_bound(&json, 0, &mut (64 << 10))?;
    Ok(json)
}

#[derive(Clone, Debug)]
enum Plan {
    Pending,
    Virtual,
    Tensors(crate::quantfmt::tensors::TensorStreamPlan),
    Blocks(crate::quantfmt::blocks::BlockStreamPlan),
}
#[derive(Clone, Debug)]
pub(super) struct Decoder {
    pub format: Arc<QuantFormat>,
    headers: Vec<Option<RoleTensor>>,
    names: Vec<Option<String>>,
    params: BTreeMap<String, i64>,
    shape: Vec<usize>,
    plan: Plan,
    metadata_inert: BTreeMap<String, Vec<String>>,
    metadata_pins: Vec<SourceRecord>,
}

pub(super) struct MetadataBudget {
    block: usize,
    remaining: usize,
    reads: usize,
    pub peak: usize,
    pub read_bytes: u64,
}
impl MetadataBudget {
    pub fn new(block: usize) -> Self {
        Self { block, remaining: 64 << 20, reads: 4_000_000, peak: 0, read_bytes: 0 }
    }
    pub fn reserve(&mut self, decoder: &Decoder) -> Result<()> {
        if !matches!(decoder.plan, Plan::Pending) {
            return Ok(());
        }
        for slot in decoder.format.as_tensors().unwrap().metadata_roles().map_err(|e| bad(e.to_string()))? {
            let bytes = decoder.headers[slot].as_ref().unwrap().stored_bytes().unwrap();
            self.remaining =
                self.remaining.checked_sub(bytes).ok_or_else(|| bad("FRONTEND_DESCRIPTOR_LIMIT: aggregate metadata bytes"))?;
            self.reads = self
                .reads
                .checked_sub(bytes.div_ceil(self.block))
                .ok_or_else(|| bad("FRONTEND_DESCRIPTOR_LIMIT: metadata read count"))?;
        }
        Ok(())
    }
}

/// Source headers only. Tensor plans that require metadata are prepared after all bindings pass.
pub(super) fn resolve(
    format: Arc<QuantFormat>,
    roles: &BTreeMap<String, String>,
    config: &Value,
    metadata_inert: &BTreeMap<String, Vec<String>>,
    target_shape: &[usize],
    source: &dyn TensorSource,
) -> Result<(Decoder, TensorMeta, BTreeMap<String, TensorMeta>)> {
    let mut sources = BTreeMap::new();
    let (headers, names, params, shape, plan) = if let Some(b) = format.as_blocks() {
        if roles.len() != 1
            || !roles.contains_key("data")
            || !metadata_inert.is_empty()
            || config.as_object().is_none_or(|o| !o.is_empty())
        {
            return Err(bad("FRONTEND_BINDING: blocks require only data role and empty config/metadata_inert"));
        }
        let (inp, leading) = target_shape.split_last().ok_or_else(|| bad("FRONTEND_BINDING: block target shape"))?;
        let rows =
            leading.iter().try_fold(1usize, |n, d| n.checked_mul(*d)).ok_or_else(|| bad("FRONTEND_BINDING: block rows overflow"))?;
        let plan = b.prepare_streamed(rows, *inp).map_err(|e| bad(e.to_string()))?;
        let name = &roles["data"];
        let meta = source.metadata(name).ok_or_else(|| bad(format!("FRONTEND_BINDING: missing tensor {name}")))?;
        let bytes = usize::try_from(meta.bytes).map_err(|_| bad("FRONTEND_BINDING: block source bytes overflow"))?;
        let byte_container =
            matches!(meta.dtype.as_str(), "U8" | "I8") && meta.shape.iter().try_fold(1usize, |n, d| n.checked_mul(*d)) == Some(bytes);
        let opaque = stored_width(&meta.dtype).is_none() && meta.shape == target_shape;
        if bytes != plan.bytes() || meta.shape.iter().any(|d| *d == 0 || *d > i64::MAX as usize) || !(byte_container || opaque) {
            return Err(bad("FRONTEND_BINDING: block source bytes/shape/storage"));
        }
        sources.insert(name.clone(), meta);
        (
            vec![Some(RoleTensor::header_only(vec![bytes], "U8"))],
            vec![Some(name.clone())],
            BTreeMap::new(),
            target_shape.to_vec(),
            Plan::Blocks(plan),
        )
    } else {
        let declared: Vec<_> =
            if let Some(t) = format.as_tensors() { t.roles().collect() } else { format.as_virtual().unwrap().roles().collect() };
        if roles.keys().any(|r| !declared.iter().any(|(name, _, _)| name == r)) {
            return Err(bad("FRONTEND_BINDING: undeclared descriptor role"));
        }
        for (role, paths) in metadata_inert {
            if paths.len() > 32
                || !format.desc.params.values().any(|p| p.from_role.as_ref().is_some_and(|p| &p.role == role))
                || !roles.contains_key(role)
            {
                return Err(bad("FRONTEND_BINDING: metadata_inert requires a declared JSON role"));
            }
            validate_paths(paths)?;
        }
        let mut headers = Vec::new();
        let mut names = Vec::new();
        for (r, _, required) in declared {
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
        let params = format.read_config(config)?.params;
        let (shape, plan) = if let Some(t) = format.as_tensors() {
            t.validate_headers(&headers).map_err(|e| bad(e.to_string()))?;
            let _bytes = t.metadata_roles().map_err(|e| bad(e.to_string()))?.into_iter().try_fold(0usize, |n, slot| {
                let header = headers[slot].as_ref().ok_or_else(|| bad("FRONTEND_BINDING: required role metadata absent"))?;
                n.checked_add(header.stored_bytes().unwrap())
                    .filter(|n| *n <= 65536)
                    .ok_or_else(|| bad("FRONTEND_DESCRIPTOR_LIMIT: role metadata exceeds 64 KiB"))
            })?;
            (target_shape.to_vec(), Plan::Pending)
        } else {
            (format.as_virtual().unwrap().shape(&headers, &params).map_err(|e| bad(e.to_string()))?, Plan::Virtual)
        };
        (headers, names, params, shape, plan)
    };
    let bytes =
        shape.iter().try_fold(4usize, |n, d| n.checked_mul(*d)).ok_or_else(|| bad("FRONTEND_BINDING: decoded shape overflow"))?;
    let meta = TensorMeta { dtype: "F32".into(), shape: shape.clone(), bytes: bytes as u64 };
    Ok((
        Decoder { format, headers, names, params, shape, plan, metadata_inert: metadata_inert.clone(), metadata_pins: Vec::new() },
        meta,
        sources,
    ))
}

impl Decoder {
    pub fn prepare(
        &mut self,
        source: &dyn TensorSource,
        sources: &BTreeMap<String, TensorMeta>,
        budget: &mut MetadataBudget,
    ) -> Result<()> {
        if !matches!(self.plan, Plan::Pending) {
            return Ok(());
        }
        let t = self.format.as_tensors().expect("pending tensor plan");
        let slots = t.metadata_roles().map_err(|e| bad(e.to_string()))?;
        for slot in slots {
            let name = self.names[slot].as_ref().expect("required metadata header");
            let meta = &sources[name];
            let bytes = usize::try_from(meta.bytes).map_err(|_| bad("FRONTEND_DESCRIPTOR_LIMIT: metadata byte count"))?;
            let mut data = Vec::with_capacity(bytes);
            for start in (0..bytes).step_by(budget.block) {
                let end = start.saturating_add(budget.block).min(bytes);
                let raw = source.read_slice(name, start as u64..end as u64)?;
                if raw.len() != end - start {
                    return Err(bad("FRONTEND_BINDING: short metadata range"));
                }
                budget.peak = budget.peak.max(raw.len());
                budget.read_bytes += raw.len() as u64;
                data.extend(raw);
            }
            let role = t.roles().nth(slot).unwrap().0;
            let paths: Vec<_> = self
                .format
                .desc
                .params
                .values()
                .filter_map(|p| p.from_role.as_ref())
                .filter(|p| p.role == role)
                .map(|p| p.path.clone())
                .collect();
            if !paths.is_empty() {
                let mut paths = paths;
                paths.extend(self.metadata_inert.get(role).cloned().unwrap_or_default());
                let json = checked_json(&data)?;
                check_paths(&json, &paths)?;
            }
            self.metadata_pins.push(source_record(name, meta, &data));
            self.headers[slot].as_mut().unwrap().data = data;
        }
        let plan = t.prepare_streamed(&self.headers, &self.params).map_err(|e| bad(e.to_string()))?;
        if plan.shape() != self.shape {
            return Err(bad("FRONTEND_BINDING: decoded descriptor shape differs from parameter"));
        }
        for header in self.headers.iter_mut().flatten() {
            header.data.clear();
            header.data.shrink_to_fit();
        }
        self.plan = Plan::Tensors(plan);
        Ok(())
    }
}

fn source_record(name: &str, meta: &TensorMeta, raw: &[u8]) -> SourceRecord {
    let mut h = blake2b_simd::Params::new().hash_length(64).key(b"MISAKA/PALW/TIR/FRONTEND/TENSOR/V1").to_state();
    h.update(canonical_json(&serde_json::json!({"dtype":meta.dtype,"shape":meta.shape})).as_bytes());
    h.update(raw);
    SourceRecord {
        name: name.to_string(),
        dtype: meta.dtype.clone(),
        shape: meta.shape.clone(),
        digest: program::hex(h.finalize().as_bytes()),
    }
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
            if range.len() > page {
                return Err(DslError("FRONTEND_STREAM_LIMIT: field exceeds role page".into()));
            }
            let aligned = range.start / page * page;
            // A block field may be unaligned and cross the nominal page boundary.
            let start = if range.end <= aligned.saturating_add(page) { aligned } else { range.start };
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
        if self.metadata_pins.iter().any(|pin| !records.contains(pin)) {
            return Err(bad("FRONTEND_BINDING: descriptor metadata changed since compilation"));
        }
        let pages = Pages {
            source,
            names: &self.names,
            headers: &self.headers,
            bytes: block / present.max(1),
            pages: RefCell::new(vec![None; self.headers.len()]),
            peak: std::cell::Cell::new(0),
            read_bytes: std::cell::Cell::new(0),
        };
        let total = self.shape.iter().product::<usize>();
        let chunk = (block / dtype.width().max(4)).min(1024);
        let mut saturated = 0;
        let mut bytes = 0;
        for at in (0..total).step_by(chunk) {
            let range = at..(at + chunk).min(total);
            let values = match &self.plan {
                Plan::Virtual => {
                    self.format
                        .as_virtual()
                        .unwrap()
                        .decode_range_streamed(&self.headers, &self.params, range, &|r, range| pages.read(r, range))
                }
                Plan::Tensors(plan) => {
                    self.format
                        .as_tensors()
                        .unwrap()
                        .decode_range_streamed(&self.headers, plan, range, &|r, range| pages.read(r, range))
                }
                Plan::Blocks(plan) => {
                    self.format.as_blocks().unwrap().decode_range_streamed(plan, range, &|range| pages.read(0, range))
                }
                Plan::Pending => return Err(bad("FRONTEND_BINDING: metadata was not prepared")),
            }
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
