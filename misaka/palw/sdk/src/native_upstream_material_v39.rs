//! Concrete source39 output→material adapter. Original trace node openings and
//! raw dtype rows are retained, rather than accepting an asserted output root.
//! Genuine upstream Final/once-credit is still reauthenticated by the native fold.
use crate::{
    native_material_v39::{CheckedMaterialFileV39, MaterialSeedV39},
    native_trace_archive_v39::DiskTraceArchiveV39,
};
use misaka_palw_kernel::{
    flat_material_output39::*,
    flat_material39::*,
    flat_point39::FlatClaimV39,
    flat_prepared39::PreparedV39,
    flat_source39::CheckedSourceV39,
    hash::{Digest, object_id},
};
use misaka_palw_tir::program_v2::OutputDecl;
use sha2::{Digest as _, Sha256};
use std::{
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};
struct CertificateDirectory {
    directory: PathBuf,
    root: Digest,
}
impl OutputCertificatePageReaderV39 for CertificateDirectory {
    fn page(&self, certificate: Digest, index: u16) -> Result<MaterialPageV39, String> {
        if certificate != self.root {
            return Err("upstream39 certificate directory root differs".into());
        }
        let raw = crate::native_audit_io_v32::read_bounded_file_v32(
            &self.directory.join(format!("certificate-page-{index:04}.borsh")),
            (32 << 10) + 2048,
        )?;
        let page = MaterialPageV39::from_bytes(&raw)?;
        if page.index != index {
            return Err("upstream39 certificate directory index differs".into());
        }
        Ok(page)
    }
}
/// Exact original Rows projection/zero padding or Final output. `seed.origin`
/// supplies the published native book/credit references, which are DATA here;
/// absence or forgery cannot be upgraded into native authority by this writer.
pub fn prepare_upstream_material_v39(
    source: &CheckedSourceV39,
    claim: &FlatClaimV39,
    archive: &DiskTraceArchiveV39,
    projection: OutputProjectionV39,
    seed: &MaterialSeedV39,
    start: u64,
    deadline: u64,
    workspace: u64,
    output: &Path,
) -> Result<serde_json::Value, String> {
    if output.exists() || claim.source != source.root() || claim.positions == 0 || claim.positions > source.original_context_max() {
        return Err("upstream39 original claim/source or fresh output differs".into());
    }
    let MaterialOriginV39::Upstream {
        master,
        source: original_source,
        claim: original_claim,
        job,
        class,
        final_book,
        single_credit,
        ..
    } = &seed.origin
    else {
        return Err("upstream39 requires original native output origin".into());
    };
    if *master != seed.master
        || *original_source != source.root()
        || *original_claim != claim.root()
        || [*job, *class, *final_book, *single_credit].contains(&[0; 64])
    {
        return Err("upstream39 original book/claim/once-credit references differ".into());
    }
    let pr = PreparedV39::new(source, claim.positions)?;
    let occurrence = u16::try_from(pr.occurrences().len() - 1).map_err(|_| "upstream39 occurrence extent")?;
    let node = source.program().output.node();
    let index = pr.index(occurrence, node)?;
    let dtype = pr.node(occurrence, node)?.out.dtype;
    let row_shape = pr.shapes(claim.positions - 1, occurrence, node)?.out;
    let row_cells = row_shape.iter().try_fold(1u64, |n, d| n.checked_mul(*d as u64)).ok_or("upstream39 original row extent")?;
    let (count, first, shape) = match (&source.program().output, &projection) {
        (OutputDecl::Rows { .. }, p) => {
            if pr.shapes(0, occurrence, node)?.out != row_shape {
                return Err("upstream39 original Rows shape changes".into());
            }
            let (from, pad) = match p {
                OutputProjectionV39::Whole => (0, None),
                OutputProjectionV39::Rows { rows_from, drop, pad_to } => {
                    (rows_from.checked_add(*drop).ok_or("upstream39 row offset overflow")?, *pad_to)
                }
            };
            let count = claim.positions.checked_sub(from).ok_or("upstream39 original projection exceeds positions")?;
            let rows = pad.unwrap_or(count);
            if rows == 0 || rows < count {
                return Err("upstream39 original projected padding extent".into());
            }
            (count, from, std::iter::once(rows).chain(row_shape.iter().map(|d| *d as u32)).collect::<Vec<_>>())
        }
        (OutputDecl::Final { .. }, OutputProjectionV39::Whole) => {
            (1, claim.positions - 1, row_shape.iter().map(|d| *d as u32).collect())
        }
        _ => return Err("upstream39 unsupported original output/projection (logits require decode contract)".into()),
    };
    let cells = shape.iter().try_fold(1u64, |n, d| n.checked_mul(*d as u64)).ok_or("upstream39 material cell extent")?;
    let bytes = cells
        .checked_mul(dtype.width() as u64)
        .filter(|n| *n > 0 && *n <= MATERIAL_MAX_RAW_V39)
        .ok_or("upstream39 material raw limit")?;
    if cells.checked_mul(16).is_none_or(|n| n > MATERIAL_MAX_EXPANDED_V39) || seed.dtype != dtype || seed.shape != shape {
        return Err("upstream39 original downstream input dtype/shape differs".into());
    }
    std::fs::create_dir(output).map_err(|e| e.to_string())?;
    let raw_path = output.join("original-output39.raw");
    let mut raw = std::fs::OpenOptions::new().read(true).write(true).create_new(true).open(&raw_path).map_err(|e| e.to_string())?;
    let cert_path = output.join("original-output-certificate39.raw");
    let mut certificate =
        std::fs::OpenOptions::new().read(true).write(true).create_new(true).open(&cert_path).map_err(|e| e.to_string())?;
    certificate
        .write_all(&39u16.to_le_bytes())
        .and_then(|_| certificate.write_all(&count.to_le_bytes()))
        .map_err(|e| e.to_string())?;
    let mut raw_sha = Sha256::new();
    let mut certificate_bytes = 6u64;
    for i in 0..count {
        let position = first + i;
        let opening = archive.node_opening(position, occurrence, node)?;
        if !opening.authenticates(&claim.segment_roots, claim.positions, index, pr.node_count()) {
            return Err("upstream39 original output opening not authenticated by accepted claim".into());
        }
        let encoded = borsh::to_vec(&opening).map_err(|e| e.to_string())?;
        certificate_bytes = certificate_bytes
            .checked_add(4 + encoded.len() as u64)
            .filter(|n| *n <= MATERIAL_MAX_RAW_V39)
            .ok_or("upstream39 certificate byte limit before write")?;
        certificate
            .write_all(&(encoded.len() as u32).to_le_bytes())
            .and_then(|_| certificate.write_all(&encoded))
            .map_err(|e| e.to_string())?;
        let mut at = 0;
        while at < row_cells {
            let n = (row_cells - at).min(4096) as usize;
            let values = archive.read_value_range(position, occurrence, node, at, n)?;
            if values.len() != n {
                return Err("upstream39 original row reader extent".into());
            }
            let mut block = Vec::with_capacity(n * dtype.width());
            for value in values {
                dtype.encode_le(value, &mut block);
            }
            raw.write_all(&block).map_err(|e| e.to_string())?;
            raw_sha.update(&block);
            at += n as u64;
        }
    }
    let written = count as u64 * row_cells * dtype.width() as u64;
    let mut remaining = bytes.checked_sub(written).ok_or("upstream39 output exceeds projected material")?;
    let zero = [0u8; 32768];
    while remaining > 0 {
        let n = remaining.min(zero.len() as u64) as usize;
        raw.write_all(&zero[..n]).map_err(|e| e.to_string())?;
        raw_sha.update(&zero[..n]);
        remaining -= n as u64;
    }
    raw.sync_all().map_err(|e| e.to_string())?;
    certificate.sync_all().map_err(|e| e.to_string())?;
    let raw_sha: [u8; 32] = raw_sha.finalize().into();
    let tensor = tensor_commitment_streamed_v39(dtype, &shape.iter().map(|n| *n as usize).collect::<Vec<_>>(), |offset, count| {
        let mut b = vec![0u8; count * dtype.width()];
        crate::tir_stream::pread(&raw, &mut b, offset.checked_mul(dtype.width() as u64).ok_or("upstream39 raw read offset")?)
            .map_err(|e| e.to_string())?;
        Ok(b.chunks_exact(dtype.width()).map(|w| dtype.decode_le(w)).collect())
    })?;
    let mut plan = OutputCertificatePlanV39 {
        version: 39,
        source: source.root(),
        claim: claim.root(),
        output_declaration: object_id(
            b"misaka-palw/kernel/original-output-declaration/v39",
            &(source.root(), &source.program().output),
        ),
        tensor,
        dtype,
        shape,
        projection,
        count,
        bytes: certificate_bytes,
        page_root: [0; 64],
    };
    certificate.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    let mut leaves = Vec::with_capacity(certificate_bytes.div_ceil(32768) as usize);
    let mut buffer = [0u8; 32768];
    loop {
        let n = certificate.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        leaves.push(output_certificate_page_leaf_v39(&plan, leaves.len() as u16, &buffer[..n]));
    }
    plan.page_root = output_certificate_page_tree_root_v39(&leaves)?;
    let mut seed = seed.clone();
    let MaterialOriginV39::Upstream { output: original_output, .. } = &mut seed.origin else { unreachable!() };
    if *original_output != [0; 64] && *original_output != plan.root() {
        return Err("upstream39 predeclared certificate root differs".into());
    }
    *original_output = plan.root();
    let material = CheckedMaterialFileV39::open(&raw_path, raw_sha, &seed, start, deadline, workspace)?;
    let checked = CheckedOutputCertificatePlanV39::check(source, claim, material.descriptor(), &plan)?;
    let prepared = output.join("prepared");
    material.write_plan(&prepared)?;
    std::fs::write(prepared.join("certificate-plan39.borsh"), plan.to_bytes()).map_err(|e| e.to_string())?;
    certificate.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    for i in 0..checked.pages() {
        let n = (plan.bytes - i as u64 * 32768).min(32768) as usize;
        certificate.read_exact(&mut buffer[..n]).map_err(|e| e.to_string())?;
        let page = MaterialPageV39 { index: i, body: buffer[..n].to_vec(), siblings: output_certificate_page_path_v39(&leaves, i)? };
        std::fs::write(prepared.join(format!("certificate-page-{i:04}.borsh")), borsh::to_vec(&page).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    }
    let certificates = CertificateDirectory { directory: prepared.clone(), root: plan.root() };
    check_upstream_material_output_v39(source, claim.root(), claim, material.descriptor(), &checked, &material, &certificates)?;
    let hex = crate::runtime_pack::commit::hex;
    let report = serde_json::json!({"schema":"misaka.palw.upstream-output-prepared.v39","source":hex(&source.root()),"claim":hex(&claim.root()),"certificate":hex(&plan.root()),"material":hex(&material.descriptor().root()),"raw_sha256":hex(&raw_sha),"prepared_directory":prepared,"native_final":"NOT_RUN","native_sealed":"NOT_RUN","a6_eligible":false,"a8_eligible":false});
    std::fs::write(output.join("upstream39.json"), serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    Ok(report)
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpstreamPreparationV39 {
    pub version: u16,
    pub source: crate::native_producer_v39::PinnedProducerFileV39,
    pub maximum: crate::native_producer_v39::PinnedProducerFileV39,
    pub materials: crate::native_producer_v39::PinnedProducerFileV39,
    pub material_directories: crate::native_producer_v39::PinnedProducerFileV39,
    pub post: crate::native_producer_v39::PinnedProducerFileV39,
    pub seed: crate::native_producer_v39::PinnedProducerFileV39,
    pub projection: crate::native_producer_v39::PinnedProducerFileV39,
    pub archive_directory: String,
    pub archive_disk_bytes: u64,
    pub start_daa: u64,
    pub deadline_daa: u64,
    pub tensor_workspace_bytes: u64,
}
pub fn prepare_from_plan_v39(
    plan_path: &Path,
    plan_sha256: [u8; 32],
    input_root: &Path,
    output: &Path,
) -> Result<serde_json::Value, String> {
    use crate::native_producer_v39::{MaterialDirectoriesV39, pinned_input, relative_file};
    use kaspa_consensus_core::palw_native_job_v39::NativeCommitWireV39;
    use misaka_palw_kernel::{
        flat_job39::{ActualInputResolverV39, CheckedJobMaximumV39, JobMaximumV39, MaterialManifestV39},
        flat_source39::{FlatSourceWireV39, SOURCE39_MAX_BYTES},
    };
    let raw = crate::native_audit_io_v32::read_bounded_file_v32(plan_path, 64 << 10)?;
    if <[u8; 32]>::from(Sha256::digest(&raw)) != plan_sha256 {
        return Err("upstream39 independently pinned preparation plan differs".into());
    }
    let p: UpstreamPreparationV39 = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
    if p.version != 39 || p.tensor_workspace_bytes == 0 || p.tensor_workspace_bytes > 256 << 20 {
        return Err("upstream39 original preparation version/workspace bound".into());
    }
    let root = input_root.canonicalize().map_err(|e| e.to_string())?;
    let source = FlatSourceWireV39::from_bytes(&pinned_input(&root, &p.source, SOURCE39_MAX_BYTES)?)?.check()?;
    let request = JobMaximumV39::from_bytes(&pinned_input(&root, &p.maximum, 8 << 20)?)?;
    let manifest = MaterialManifestV39::from_bytes(&pinned_input(&root, &p.materials, 64 << 10)?)?;
    let maximum = CheckedJobMaximumV39::check(&source, &request, &manifest)?;
    let map = serde_json::from_slice(&pinned_input(&root, &p.material_directories, 64 << 10)?).map_err(|e| e.to_string())?;
    let materials = MaterialDirectoriesV39::open(&map, &manifest, &root)?;
    let post = NativeCommitWireV39::from_bytes(&pinned_input(&root, &p.post, 9 << 20)?, maximum.maximum_positions())?;
    let actual = ActualInputResolverV39::derive(&source, &maximum, &post.generated, &materials)?;
    let seed = MaterialSeedV39::from_bytes(&pinned_input(&root, &p.seed, 2048)?)?;
    let MaterialOriginV39::Upstream { job, .. } = &seed.origin else {
        return Err("upstream39 original origin absent".into());
    };
    if *job != post.job {
        return Err("upstream39 original funded job differs from pinned post".into());
    }
    let projection: OutputProjectionV39 = borsh::from_slice(&pinned_input(&root, &p.projection, 64)?).map_err(|e| e.to_string())?;
    let archive = DiskTraceArchiveV39::open_existing(
        &relative_file(&root, &p.archive_directory)?,
        &source,
        &maximum,
        &actual,
        &post.claim,
        p.archive_disk_bytes,
    )?;
    let mut report = prepare_upstream_material_v39(
        &source,
        &post.claim,
        &archive,
        projection,
        &seed,
        p.start_daa,
        p.deadline_daa,
        p.tensor_workspace_bytes,
        output,
    )?;
    report["preparation_plan_sha256"] = serde_json::Value::String(crate::runtime_pack::commit::hex(&plan_sha256));
    std::fs::write(output.join("upstream39.json"), serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    Ok(report)
}
