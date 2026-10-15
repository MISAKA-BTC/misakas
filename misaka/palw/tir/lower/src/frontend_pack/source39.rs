//! Explicit public frontend39. Canonical39 program bytes are a strict config
//! expression (hex), while actual tensor imports use the generic bounded IEEE,
//! integer and packed descriptor converters. No old program/admission is built.
use super::*;
use misaka_palw_tir::program_v39::TirProgramV39;
pub const MAX_PACK_BYTES_V39: usize = 40 << 20;
pub const FORMAT_V39: &str = "misaka.palw.tir-frontend.v39";
pub struct FrontendPackV39 {
    definition: Definition,
    hash: String,
    pub(super) formats: BTreeMap<String, std::sync::Arc<crate::quantfmt::QuantFormat>>,
}
pub struct CompiledSourceV39 {
    program: TirProgramV39,
    bindings: BTreeMap<(u16, Option<u16>), Resolved>,
    metadata_read_bytes: u64,
    metadata_max_read_bytes: usize,
    pack_hash: String,
    config_hash: String,
    assumed_defaults: Vec<String>,
}
impl FrontendPackV39 {
    pub fn parse(text: &str) -> Result<Self> {
        if text.len() > MAX_PACK_BYTES_V39 {
            return Err(bad("FRONTEND39_PACK_BYTES"));
        }
        let value: Value = serde_json::from_str(text).map_err(|e| bad(e.to_string()))?;
        let mut remaining = crate::adapter::expr::MAX_EXPANSION_BYTES;
        input_bound(&value, 0, &mut remaining)?;
        let hash = digest(b"MISAKA/PALW/TIR/FRONTEND/PACK/V39", canonical_json(&value).as_bytes());
        let definition: Definition = serde_json::from_value(value).map_err(|e| bad(e.to_string()))?;
        if definition.format != FORMAT_V39
            || definition.id.is_empty()
            || definition.id.len() > 128
            || definition.scope.task.is_empty()
            || definition.scope.task.len() > 128
            || definition.scope.components.len() > 64
            || definition.scope.components.iter().any(|n| n.is_empty() || n.len() > 128)
            || definition.vars.len() > 4096
            || definition.vars.keys().any(|n| n.is_empty() || n.len() > 128)
            || definition.quant_formats.len() > descriptor::MAX_FORMATS
        {
            return Err(bad("FRONTEND39_DEFINITION_BOUNDS"));
        }
        let mut formats = BTreeMap::new();
        let mut test_work = 0;
        for (id, v) in &definition.quant_formats {
            if id.is_empty() || id.len() > 128 {
                return Err(bad("FRONTEND39_FORMAT_ID"));
            }
            formats.insert(id.clone(), descriptor::parse(v, &mut test_work)?);
        }
        Ok(Self { definition, hash, formats })
    }
    pub fn scope(&self) -> &Scope {
        &self.definition.scope
    }
    pub fn hash(&self) -> &str {
        &self.hash
    }
    pub fn compile(&self, config: &Value, source: &dyn TensorSource, block_bytes: usize) -> Result<CompiledSourceV39> {
        if !(8..=stream::MAX_BLOCK_BYTES).contains(&block_bytes) {
            return Err(bad("FRONTEND39_STREAM_BUDGET"));
        }
        source.validate_snapshot()?;
        let d = &self.definition;
        let map = config.as_object().ok_or_else(|| bad("FRONTEND39_CONFIG_OBJECT"))?;
        let mut remaining = MAX_CONFIG_BYTES;
        input_bound(config, 0, &mut remaining)?;
        let names = source.names();
        if names.len() > MAX_SOURCE_TENSORS {
            return Err(bad("FRONTEND39_SOURCE_COUNT"));
        }
        let available = names.iter().cloned().collect::<BTreeSet<_>>();
        if available.len() != names.len() {
            return Err(bad("FRONTEND39_DUPLICATE_SOURCE"));
        }
        let index = TensorIndex::from_source(source);
        let cfg = Cfg::new_strict(&d.id, map, "");
        cfg.inert(&d.inert.iter().map(String::as_str).collect::<Vec<_>>());
        let env = Env::new(&cfg, None, &d.defaults, Some(&index), &d.id);
        for (n, e) in &d.vars {
            env.define_global(n, e.clone());
        }
        env.force_globals(&d.vars.keys().cloned().collect::<Vec<_>>())?;
        let evaluated = env.eval(&d.program)?;
        let encoded = evaluated.as_str().ok_or_else(|| bad("FRONTEND39_PROGRAM_IS_CANONICAL39_HEX_EXPRESSION"))?;
        let raw = program::unhex(encoded, 16 << 20)?;
        let program = TirProgramV39::decode_canonical(&raw).map_err(bad)?;
        let bindings: Vec<Binding> =
            serde_json::from_value(env.eval(&d.bindings)?).map_err(|e| bad(format!("FRONTEND_BINDING: {e}")))?;
        cfg.finish()?;
        let assumed_defaults = env.assumed.borrow().iter().cloned().collect();
        let expected: BTreeSet<_> = program
            .params
            .iter()
            .enumerate()
            .flat_map(|(j, d)| {
                if d.per_layer {
                    (0..program.schedule.layers.len()).map(|l| (j as u16, Some(l as u16))).collect::<Vec<_>>()
                } else {
                    vec![(j as u16, None)]
                }
            })
            .collect();
        if expected.len() > 16384 {
            return Err(bad("FRONTEND39_PARAMETER_INSTANCE_CAP"));
        }
        let mut resolved = BTreeMap::new();
        let mut used = BTreeSet::new();
        let mut used_formats = BTreeSet::new();
        for b in bindings {
            if !expected.contains(&(b.param, b.layer)) || resolved.contains_key(&(b.param, b.layer)) {
                return Err(bad("FRONTEND_BINDING: unexpected or repeated parameter instance"));
            }
            let target_shape: Vec<_> = program.params[b.param as usize].shape.iter().map(|n| *n as usize).collect();
            let (meta, sources, decoder) = if let Import::Descriptor { format, roles, config, metadata_inert, shift, .. } = &b.import {
                if !b.source.is_empty() || !(-64..=64).contains(shift) {
                    return Err(bad("FRONTEND_BINDING: descriptor uses roles only and shift -64..=64"));
                }
                let format = self.formats.get(format).ok_or_else(|| bad("FRONTEND_BINDING: undeclared descriptor"))?.clone();
                let (decoder, meta, sources) = descriptor::resolve(format, roles, config, metadata_inert, &target_shape, source)?;
                if let Import::Descriptor { format, .. } = &b.import {
                    used_formats.insert(format.clone());
                }
                (meta, sources, Some(decoder))
            } else {
                let meta = source.metadata(&b.source).ok_or_else(|| bad(format!("FRONTEND_BINDING: missing tensor {}", b.source)))?;
                (meta.clone(), BTreeMap::from([(b.source.clone(), meta)]), None)
            };
            let param = &program.params[b.param as usize];
            if meta.shape != param.shape.iter().map(|n| *n as usize).collect::<Vec<_>>() {
                return Err(bad(format!("FRONTEND_BINDING: shape of {}", b.source)));
            }
            let width = stored_width(&meta.dtype)
                .ok_or_else(|| LowerError::not_lowerable(format!("SOURCE_FORMAT_UNSUPPORTED: {}", meta.dtype)))?;
            let count = meta
                .shape
                .iter()
                .try_fold(1u64, |n, d| n.checked_mul(*d as u64))
                .ok_or_else(|| bad("FRONTEND_BINDING: shape overflow"))?;
            if count.checked_mul(width as u64) != Some(meta.bytes) {
                return Err(bad("FRONTEND_BINDING: source byte count"));
            }
            match b.import {
                Import::Integer if !is_integer(&meta.dtype) => {
                    return Err(bad("FRONTEND_BINDING: integer import requires integer storage"));
                }
                Import::FixedPoint { shift, .. } if is_integer(&meta.dtype) || !(-64..=64).contains(&shift) => {
                    return Err(bad("FRONTEND_BINDING: fixed point requires IEEE storage and shift -64..=64"));
                }
                _ => {}
            }
            used.extend(sources.keys().cloned());
            resolved.insert((b.param, b.layer), Resolved { binding: b, meta, sources, decoder });
        }
        if resolved.len() != expected.len() {
            return Err(bad("FRONTEND_BINDING: missing parameter instance"));
        }
        if used != available {
            return Err(LowerError::not_lowerable(format!("TENSOR_UNREAD: {:?}", available.difference(&used).collect::<Vec<_>>())));
        }
        if used_formats.len() != self.formats.len() {
            return Err(bad("FRONTEND_DESCRIPTOR: unused format"));
        }
        let mut metadata_budget = descriptor::MetadataBudget::new(block_bytes);
        for r in resolved.values() {
            if let Some(decoder) = &r.decoder {
                metadata_budget.reserve(decoder)?;
            }
        }
        for r in resolved.values_mut() {
            if let Some(decoder) = &mut r.decoder {
                decoder.prepare(source, &r.sources, &mut metadata_budget)?;
            }
        }

        Ok(CompiledSourceV39 {
            program,
            bindings: resolved,
            metadata_read_bytes: metadata_budget.read_bytes,
            metadata_max_read_bytes: metadata_budget.peak,
            pack_hash: self.hash.clone(),
            config_hash: digest(b"MISAKA/PALW/TIR/FRONTEND/CONFIG/V39", canonical_json(config).as_bytes()),
            assumed_defaults,
        })
    }
}
impl CompiledSourceV39 {
    pub fn program(&self) -> &TirProgramV39 {
        &self.program
    }
    pub fn source_names(&self) -> BTreeSet<String> {
        self.bindings.values().flat_map(|r| r.sources.keys().cloned()).collect()
    }
    pub fn provenance(&self) -> Value {
        serde_json::json!({"schema":"misaka.palw.public-frontend-compiler.v39","pack_hash":self.pack_hash,"config_hash":self.config_hash,"assumed_defaults":self.assumed_defaults})
    }
    pub fn write_tensor(
        &self,
        source: &dyn TensorSource,
        j: u16,
        l: Option<u16>,
        out: &mut dyn std::io::Write,
        block_bytes: usize,
    ) -> Result<super::stage28::StageTensorReportV28> {
        if !(8..=stream::MAX_BLOCK_BYTES).contains(&block_bytes) || !self.bindings.contains_key(&(j, l)) {
            return Err(bad("FRONTEND_STAGE28_TENSOR_OR_BUDGET"));
        }
        source.validate_snapshot()?;
        let mut records = BTreeMap::<String, SourceRecord>::new();
        let mut saturated_values = 0u64;
        let mut tensor_bytes = 0u64;
        let mut max_read_bytes = self.metadata_max_read_bytes;
        let mut source_read_bytes = self.metadata_read_bytes;
        let mut quant_formats = BTreeMap::new();
        let result = (|| -> std::result::Result<(), String> {
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
                        Import::Integer => stream::integer(&r.meta.dtype, chunk),
                        Import::FixedPoint { shift, round, .. } => {
                            stream::ieee_integer(&r.meta.dtype, chunk, shift, round.compile()).map_err(|e| e.to_string())?
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
        })()
        .map_err(LowerError::weights);
        result?;
        source.validate_snapshot()?;
        Ok(super::stage28::StageTensorReportV28 {
            tensor_bytes,
            source_read_bytes,
            max_read_bytes,
            saturated_values,
            source_tensors: records.into_values().collect(),
            quant_formats,
        })
    }
}
