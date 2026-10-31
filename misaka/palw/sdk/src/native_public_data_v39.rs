//! Concrete original39 public responder. Trace directories come from the paid
//! producer archive; parameter/input trees are materialised once into a bounded
//! local cache. Hash/Merkle openings authenticate identity, not correctness.
use crate::{native_tensor_store_v39::DiskTensorV39, native_trace_archive_v39::DiskTraceArchiveV39};
use misaka_palw_kernel::{flat_detect39::PublicReadsV39, flat_point39::CheckedPointContextV39, flat_public39::*, hash::object_id};
use misaka_palw_tir::interp_v39::ParamSourceV39;
use std::path::{Path, PathBuf};
pub struct PublicDataV39<'a> {
    context: &'a CheckedPointContextV39<'a>,
    parameters: &'a dyn ParamSourceV39,
    trace: &'a DiskTraceArchiveV39,
    root: PathBuf,
    budget: u64,
    stored: u64,
}
impl<'a> PublicDataV39<'a> {
    pub fn create(
        path: &Path,
        context: &'a CheckedPointContextV39<'a>,
        parameters: &'a dyn ParamSourceV39,
        trace: &'a DiskTraceArchiveV39,
        maximum_disk_bytes: u64,
    ) -> Result<Self, String> {
        if maximum_disk_bytes == 0 {
            return Err("publicdata39 explicit disk budget absent".into());
        }
        std::fs::create_dir(path).map_err(|e| e.to_string())?;
        let root = path.canonicalize().map_err(|e| e.to_string())?;
        Ok(Self { context, parameters, trace, root, budget: maximum_disk_bytes, stored: 0 })
    }
    pub fn stored_bytes(&self) -> u64 {
        self.stored
    }
    fn tensor(&mut self, key: &ValueKeyV39) -> Result<DiskTensorV39, String> {
        let source = self.context.prepared().source();
        let (dtype, shape, expected, maximum) = match *key {
            ValueKeyV39::Parameter { param, layer } => {
                let d = source.program().params.get(param as usize).ok_or("publicdata39 parameter declaration absent")?;
                let root = *source.parameters().by_instance.get(&(param, layer)).ok_or("publicdata39 original PC instance absent")?;
                (d.dtype, d.shape.iter().map(|d| *d as usize).collect::<Vec<_>>(), root, source.profile().max_parameter_elements)
            }
            ValueKeyV39::Input { position, input } => {
                let d = source.program().inputs.get(input as usize).ok_or("publicdata39 original input declaration absent")?;
                (
                    d.dtype,
                    d.shape.iter().map(|d| *d as usize).collect::<Vec<_>>(),
                    self.context.inputs().input_root(position, input)?,
                    source.profile().max_value_elements,
                )
            }
            _ => return Err("publicdata39 trace must use original trace archive".into()),
        };
        let name = crate::runtime_pack::commit::hex(&object_id(
            b"misaka-palw/sdk/public-data-cache-key/v39",
            &(source.root(), self.context.claim().root(), key),
        ));
        let path = self.root.join(name);
        if path.exists() {
            return DiskTensorV39::open_existing(&path, expected, maximum, self.budget);
        }
        let remaining = self.budget.checked_sub(self.stored).ok_or("publicdata39 original cache budget exhausted")?;
        let tensor = DiskTensorV39::create(&path, dtype, &shape, expected, maximum, remaining, |offset, count| match *key {
            ValueKeyV39::Parameter { param, layer } => {
                self.parameters.read_range(param, layer, offset, count).map_err(|e| e.to_string())
            }
            ValueKeyV39::Input { position, input } => self.context.inputs().read_input_range(position, input, offset, count),
            _ => Err("publicdata39 unsupported value key".into()),
        })?;
        self.stored = self
            .stored
            .checked_add(tensor.stored_bytes())
            .filter(|n| *n <= self.budget)
            .ok_or("publicdata39 original cumulative cache budget")?;
        Ok(tensor)
    }
    pub fn public_reply(&mut self, demand: &FlatDemandV39) -> Result<Option<FlatReplyV39>, String> {
        check_flat_demand_v39(self.context, demand)?;
        let reply = if let Some(reply) = self.trace.public_reply(demand)? {
            reply
        } else {
            match demand {
                FlatDemandV39::Header { value } => FlatReplyV39::Header(self.tensor(value)?.header()),
                FlatDemandV39::Leaf { value, axis, line, tile } => {
                    FlatReplyV39::Leaf { node: None, leaf: self.tensor(value)?.leaf(*axis, *line, *tile)? }
                }
                FlatDemandV39::HeaderLeaf { value, header, axis, line, tile } => {
                    let t = self.tensor(value)?;
                    if t.header() != *header {
                        return Err("publicdata39 supplied typed header differs".into());
                    }
                    FlatReplyV39::Leaf { node: None, leaf: t.leaf(*axis, *line, *tile)? }
                }
                FlatDemandV39::Branch { directory: DirectoryV39::Tensor { value, header, axis }, first, count, .. } => {
                    let t = self.tensor(value)?;
                    if t.header() != *header || *count < 2 {
                        return Err("publicdata39 original branch/header differs".into());
                    }
                    let left = 1u64 << (63 - (*count - 1).leading_zeros());
                    FlatReplyV39::Children {
                        left: t.subtree(*axis, *first, left)?,
                        right: t.subtree(*axis, first.checked_add(left).ok_or("publicdata39 subtree offset")?, count - left)?,
                    }
                }
                _ => return Ok(None),
            }
        };
        check_flat_reply_v39(self.context, demand, &reply)?;
        Ok(Some(reply))
    }
}
impl PublicReadsV39 for PublicDataV39<'_> {
    fn reply(&mut self, d: &FlatDemandV39) -> Result<Option<Vec<u8>>, String> {
        self.public_reply(d).map(|r| r.map(|r| r.to_bytes()))
    }
}
