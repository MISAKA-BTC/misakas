//! Actual reference39 producer. Decode is computed online from original typed
//! parameters/materials. Only then are actual firstEOS inputs bound and the
//! full all-node trace independently reconstructed. No future padding IDs or
//! producer DTO can fabricate a native Final through this API.
use misaka_palw_kernel::{
    flat_detect39::IndependentArchiveV39,
    flat_job39::{
        ActualInputResolverV39, CheckedJobMaximumV39, GenerationInputProviderV39, InputBindingV39, MaterialManifestV39, RequestStopV39,
    },
    flat_material39::{MaterialDescriptorV39, MaterialPageReaderV39, MaterialPageV39},
    flat_source39::CheckedSourceV39,
    flat_trace39::{TraceSummaryV39, trace_streamed_v39},
    hash::Digest,
};
use misaka_palw_tir::{
    interp::RunState,
    interp_v39::{InterpreterV39, ParamSourceV39},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};
pub struct ProducedReferenceV39 {
    pub generated: Vec<u32>,
    pub trace: TraceSummaryV39,
}
pub fn produce_reference_v39<A: IndependentArchiveV39>(
    source: &CheckedSourceV39,
    maximum: &CheckedJobMaximumV39,
    materials: &dyn MaterialPageReaderV39,
    parameters: &dyn ParamSourceV39,
    archive: &mut A,
) -> Result<ProducedReferenceV39, String> {
    let mut generated = Vec::new();
    if let RequestStopV39::Decode { prompt, requested_new_tokens, eos } = &maximum.request().stop {
        let provider = GenerationInputProviderV39::derive(source, maximum, materials)?;
        let interpreter = InterpreterV39::new(source.program()).map_err(|e| e.to_string())?;
        let mut state = RunState::default();
        let mut last = None;
        for token in prompt {
            let output = interpreter
                .step_with_observer(parameters, &provider, &mut state, *token, &mut |_| Ok(()))
                .map_err(|e| e.to_string())?;
            last = Some(output);
        }
        let mut logits = last.ok_or("producer39 original prompt is empty")?;
        for index in 0..*requested_new_tokens {
            if logits.data.len() != source.program().token_bound as usize {
                return Err("producer39 original logits vocabulary extent".into());
            }
            let selected = logits
                .data
                .iter()
                .enumerate()
                .max_by(|(i, a), (j, b)| a.cmp(b).then_with(|| j.cmp(i)))
                .map(|(i, _)| i as u32)
                .ok_or("producer39 original logits are empty")?;
            generated.push(selected);
            if eos.binary_search(&selected).is_ok() || index + 1 == *requested_new_tokens {
                break;
            }
            logits = interpreter
                .step_with_observer(parameters, &provider, &mut state, selected, &mut |_| Ok(()))
                .map_err(|e| e.to_string())?;
        }
    }
    let actual = ActualInputResolverV39::derive(source, maximum, &generated, materials)?;
    archive.begin(source.root(), maximum.root(), actual.root())?;
    let trace = trace_streamed_v39(source, maximum, &actual, parameters, archive)?;
    archive.finish(&trace)?;
    // Greedy selection is checked again at the original prediction positions by
    // trace_streamed39, including the first EOS and exact source input binding.
    if trace.claim.input_binding != actual.root() || trace.claim.positions as usize != actual.tokens().len() {
        return Err("producer39 actual trace/input binding differs".into());
    }
    Ok(ProducedReferenceV39 { generated, trace })
}
/// A concrete local page store, bound to the immutable original input manifest.
/// Every returned page is authenticated again by the shared39 input resolver.
pub struct MaterialDirectoriesV39 {
    directories: BTreeMap<Digest, PathBuf>,
}
impl MaterialDirectoriesV39 {
    pub fn open(mapping: &BTreeMap<String, String>, manifest: &MaterialManifestV39, root: &Path) -> Result<Self, String> {
        let root = root.canonicalize().map_err(|e| e.to_string())?;
        let mut expected = BTreeSet::new();
        let mut directories = BTreeMap::new();
        for binding in &manifest.bindings {
            if let InputBindingV39::Material { descriptor, .. } = binding {
                let id = descriptor.root();
                if !expected.insert(id) {
                    continue;
                }
                let name = crate::runtime_pack::commit::hex(&id);
                let relative = mapping.get(&name).ok_or("producer39 original material directory missing")?;
                let directory = root.join(relative).canonicalize().map_err(|e| e.to_string())?;
                if !directory.starts_with(&root) {
                    return Err("producer39 material path escapes pinned root".into());
                }
                let raw = crate::native_audit_io_v32::read_bounded_file_v32(&directory.join("descriptor39.borsh"), 2048)?;
                let stored = MaterialDescriptorV39::from_bytes(&raw)?;
                if stored != *descriptor {
                    return Err("producer39 stored material descriptor differs from original maximum".into());
                }
                directories.insert(id, directory);
            }
        }
        if expected.len() != mapping.len() {
            return Err("producer39 material directory set has unbound entries".into());
        }
        Ok(Self { directories })
    }
}
impl MaterialPageReaderV39 for MaterialDirectoriesV39 {
    fn page(&self, descriptor: Digest, index: u16) -> Result<MaterialPageV39, String> {
        let directory = self.directories.get(&descriptor).ok_or("producer39 material descriptor absent")?;
        let path = directory.join(format!("page-{index:04}.borsh"));
        let raw = crate::native_audit_io_v32::read_bounded_file_v32(&path, (32 << 10) + 2048)?;
        let page = MaterialPageV39::from_bytes(&raw)?;
        if page.index != index {
            return Err("producer39 original material page index differs".into());
        }
        Ok(page)
    }
}
