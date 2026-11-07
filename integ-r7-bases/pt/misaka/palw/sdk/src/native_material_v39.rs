//! Bounded physical input preparation for owning39. A file and its page proofs
//! are data, not native Final/once-credit authority. Upstream origins must be
//! reauthenticated by the Master39/native material fold before a funded use.
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::{flat_material39::*, hash::Digest, merkle3_stream::tensor_commitment_streamed_v3};
use sha2::{Digest as _, Sha256};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
    sync::Mutex,
};
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct MaterialSeedV39 {
    pub version: u16,
    pub source: Digest,
    pub master: Digest,
    pub requester: Digest,
    pub requester_operator: Digest,
    pub nonce: Digest,
    pub input: u8,
    pub dtype: misaka_palw_tir::DType,
    pub shape: Vec<u32>,
    pub origin: MaterialOriginV39,
}
pub struct CheckedMaterialFileV39 {
    file: Mutex<File>,
    sha256: [u8; 32],
    leaves: Vec<Digest>,
    plan: CheckedMaterialPlanV39,
}
impl MaterialSeedV39 {
    pub fn from_bytes(raw: &[u8]) -> Result<Self, String> {
        if raw.len() > 2048 {
            return Err("material39 seed allocation extent".into());
        }
        let mut r = std::io::Cursor::new(raw);
        let version = u16::deserialize_reader(&mut r).map_err(|_| "material39 seed version")?;
        let source = Digest::deserialize_reader(&mut r).map_err(|_| "material39 seed source")?;
        let master = Digest::deserialize_reader(&mut r).map_err(|_| "material39 seed master")?;
        let requester = Digest::deserialize_reader(&mut r).map_err(|_| "material39 seed requester")?;
        let requester_operator = Digest::deserialize_reader(&mut r).map_err(|_| "material39 seed operator")?;
        let nonce = Digest::deserialize_reader(&mut r).map_err(|_| "material39 seed nonce")?;
        let input = u8::deserialize_reader(&mut r).map_err(|_| "material39 seed input")?;
        let dtype = misaka_palw_tir::DType::deserialize_reader(&mut r).map_err(|_| "material39 seed dtype")?;
        let n = u32::deserialize_reader(&mut r).map_err(|_| "material39 seed rank")? as usize;
        if n > 4 {
            return Err("material39 seed rank cap".into());
        }
        let mut shape = Vec::with_capacity(n);
        for _ in 0..n {
            shape.push(u32::deserialize_reader(&mut r).map_err(|_| "material39 seed axis")?);
        }
        let origin = MaterialOriginV39::deserialize_reader(&mut r).map_err(|_| "material39 seed origin")?;
        let seed = Self { version, source, master, requester, requester_operator, nonce, input, dtype, shape, origin };
        if version != 39 || r.position() != raw.len() as u64 || borsh::to_vec(&seed).map_err(|e| e.to_string())? != raw {
            return Err("material39 seed canonical grammar".into());
        }
        Ok(seed)
    }
}
impl CheckedMaterialFileV39 {
    /// The raw source SHA is independently pinned before native payment. Only
    /// per-page buffers and the bounded original hash directory are retained.
    pub fn open(
        path: &Path,
        sha256: [u8; 32],
        seed: &MaterialSeedV39,
        start: u64,
        deadline: u64,
        workspace: u64,
    ) -> Result<Self, String> {
        let cells = seed.shape.iter().try_fold(1u64, |n, d| n.checked_mul(*d as u64)).ok_or("material39 seed tensor product")?;
        let bytes = cells
            .checked_mul(seed.dtype.width() as u64)
            .filter(|n| *n > 0 && *n <= MATERIAL_MAX_RAW_V39)
            .ok_or("material39 raw byte extent")?;
        if seed.version != 39
            || seed.shape.len() > 4
            || seed.shape.contains(&0)
            || cells.checked_mul(16).is_none_or(|n| n > MATERIAL_MAX_EXPANDED_V39)
            || workspace == 0
            || workspace > 256 << 20
            || deadline <= start
            || [seed.source, seed.master, seed.requester, seed.requester_operator].contains(&[0; 64])
        {
            return Err("material39 seed explicit memory extent".into());
        }
        let mut file = File::open(path).map_err(|e| e.to_string())?;
        if file.metadata().map_err(|e| e.to_string())?.len() != bytes {
            return Err("material39 raw source exact length".into());
        }
        let mut digest = Sha256::new();
        let mut block = [0u8; 32768];
        loop {
            let n = file.read(&mut block).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            digest.update(&block[..n]);
        }
        if <[u8; 32]>::from(digest.finalize()) != sha256 {
            return Err("material39 raw independently pinned SHA differs".into());
        }
        let shape = seed.shape.iter().map(|n| *n as usize).collect::<Vec<_>>();
        let geometry = misaka_palw_kernel::merkle3_stream::streamed_commitment_shape_v3(seed.dtype, &shape)?;
        let tensor = if geometry.workspace_bytes <= workspace {
            tensor_commitment_streamed_v3(seed.dtype, &shape, workspace, &mut |at, out| {
                file.seek(SeekFrom::Start(at)).map_err(|e| e.to_string())?;
                file.read_exact(out).map_err(|e| e.to_string())
            })?
            .0
        } else {
            if workspace < 256 << 10 {
                return Err("material39 explicit frontier leaf workspace".into());
            }
            // A large RGB raster has one original row-tree leaf per pixel.
            // Frontier39 retains O(log leaves), preserving the exact4096-element
            // v3 leaf geometry. Its extra original read calls remain chargeable.
            misaka_palw_kernel::flat_material39::tensor_commitment_streamed_v39(seed.dtype, &shape, |offset, count| {
                if count > 4096 {
                    return Err("material39 original leaf read count".into());
                }
                let width = seed.dtype.width();
                let size = count.checked_mul(width).ok_or("material39 original leaf raw bytes")?;
                let at = offset.checked_mul(width as u64).ok_or("material39 original leaf byte offset")?;
                let mut raw = vec![0; size];
                file.seek(SeekFrom::Start(at)).map_err(|e| e.to_string())?;
                file.read_exact(&mut raw).map_err(|e| e.to_string())?;
                Ok(raw.chunks_exact(width).map(|b| seed.dtype.decode_le(b)).collect())
            })?
        };
        if geometry.tensor_bytes != bytes {
            return Err("material39 original C_v3 byte geometry differs".into());
        }
        let mut descriptor = MaterialDescriptorV39 {
            version: 39,
            source: seed.source,
            master: seed.master,
            requester: seed.requester,
            requester_operator: seed.requester_operator,
            nonce: seed.nonce,
            input: seed.input,
            dtype: seed.dtype,
            shape: seed.shape.clone(),
            tensor,
            page_root: [0; 64],
            origin: seed.origin.clone(),
        };
        let count = bytes.div_ceil(MATERIAL_PAGE_BYTES_V39 as u64) as usize;
        if count > MATERIAL_MAX_PAGES_V39 as usize {
            return Err("material39 raw page count cap".into());
        }
        let mut leaves = Vec::with_capacity(count);
        file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        let mut digest = Sha256::new();
        let mut at = 0u64;
        for i in 0..count {
            let n = (bytes - at).min(MATERIAL_PAGE_BYTES_V39 as u64) as usize;
            file.read_exact(&mut block[..n]).map_err(|e| e.to_string())?;
            digest.update(&block[..n]);
            leaves.push(material_page_leaf_v39(&descriptor, i as u16, &block[..n]));
            at += n as u64;
        }
        if <[u8; 32]>::from(digest.finalize()) != sha256 || file.metadata().map_err(|e| e.to_string())?.len() != bytes {
            return Err("material39 raw changed between C_v3 and original page tree".into());
        }
        descriptor.page_root = material_page_tree_root_v39(&leaves)?;
        let plan = CheckedMaterialPlanV39::derive(&descriptor, start, deadline)?;
        Ok(Self { file: Mutex::new(file), sha256, leaves, plan })
    }
    pub fn plan(&self) -> &CheckedMaterialPlanV39 {
        &self.plan
    }
    pub fn descriptor(&self) -> &MaterialDescriptorV39 {
        self.plan.descriptor()
    }
    pub fn sha256(&self) -> [u8; 32] {
        self.sha256
    }
    pub fn page(&self, index: u16) -> Result<MaterialPageV39, String> {
        let n = self.plan.part_bytes(index)?;
        let mut file = self.file.lock().map_err(|_| "material39 source reader lock")?;
        file.seek(SeekFrom::Start(index as u64 * MATERIAL_PAGE_BYTES_V39 as u64)).map_err(|e| e.to_string())?;
        let mut body = vec![0; n];
        file.read_exact(&mut body).map_err(|e| e.to_string())?;
        if material_page_leaf_v39(self.descriptor(), index, &body)
            != *self.leaves.get(index as usize).ok_or("material39 page index")?
        {
            return Err("material39 raw page changed after independent source pin".into());
        }
        Ok(MaterialPageV39 { index, body, siblings: material_page_path_v39(&self.leaves, index)? })
    }
    pub fn write_plan(&self, out: &Path) -> Result<(), String> {
        if out.exists() {
            return Err("material39 fresh output required".into());
        }
        std::fs::create_dir(out).map_err(|e| e.to_string())?;
        for (name, bytes) in [
            ("descriptor39.borsh", self.descriptor().to_bytes()),
            ("plan39.borsh", borsh::to_vec(self.plan.record()).map_err(|e| e.to_string())?),
        ] {
            File::create(out.join(name)).map_err(|e| e.to_string())?.write_all(&bytes).map_err(|e| e.to_string())?;
        }
        for index in 0..self.plan.record().pages {
            let page = self.page(index)?;
            let mut raw = Vec::with_capacity(10 + page.body.len() + page.siblings.len() * 64);
            raw.extend_from_slice(&page.index.to_le_bytes());
            raw.extend_from_slice(&(page.body.len() as u32).to_le_bytes());
            raw.extend_from_slice(&page.body);
            raw.extend_from_slice(&(page.siblings.len() as u32).to_le_bytes());
            for root in page.siblings {
                raw.extend_from_slice(&root);
            }
            MaterialPageV39::from_bytes(&raw)?;
            File::create(out.join(format!("page-{index:04}.borsh")))
                .map_err(|e| e.to_string())?
                .write_all(&raw)
                .map_err(|e| e.to_string())?;
        }
        let hex = crate::runtime_pack::commit::hex;
        let report = serde_json::json!({"schema":"misaka.palw.actual-material-file.v39","descriptor":hex(&self.descriptor().root()),"plan":hex(&self.plan.root()),"original_raw_sha256":hex(&self.sha256),"pages":self.plan.record().pages,"tensor":hex(&self.descriptor().tensor),"page_root":hex(&self.descriptor().page_root),"native_sealed":"NOT_RUN","native_final":"NOT_RUN","a6_eligible":false});
        File::create(out.join("material39.json"))
            .map_err(|e| e.to_string())?
            .write_all(&serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}
impl MaterialPageReaderV39 for CheckedMaterialFileV39 {
    fn page(&self, descriptor: Digest, index: u16) -> Result<MaterialPageV39, String> {
        if descriptor != self.descriptor().root() {
            return Err("material39 another original descriptor".into());
        }
        self.page(index)
    }
}
