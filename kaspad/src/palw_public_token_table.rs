//! Bounded-memory PALWTTB1 importer and authenticated random-access public operand.
use crate::palw_memory_ledger::{PalwMemoryReservationKeyV1, PalwMemoryReservationV1, host_ledger_v1};
use kaspa_consensus_core::{
    Hash64,
    palw_fp_constraint_job_v1::{
        PALW_TOKEN_TABLE_DOMAIN_LEAF_V1, PALW_TOKEN_TABLE_FILE_MAGIC_V1, PALW_TOKEN_TABLE_MAX_LEAVES_V1, table_eog_leaf_hash_v1,
    },
    palw_public_token_table_v1::{PALW_PUBLIC_TABLE_PART_BYTES_V1, PalwPublicTokenTableSourceV1},
    palw_step_leg::{PalwStepOpeningV1, step_merkle_leaf_v1, step_merkle_node_v1},
};
use std::{
    fs::File,
    io::{BufReader, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

const RECORD: u64 = 80;
pub(crate) struct FileTableV1 {
    original: Mutex<File>,
    index: Mutex<File>,
    vocab: u32,
    tokenizer: Hash64,
    root: Hash64,
    eog_count: u32,
    eog_offset: u64,
    levels: Vec<(u64, u64)>,
    _holding: PalwMemoryReservationV1,
}
fn read_u32(r: &mut impl Read) -> Result<u32, String> {
    let mut b = [0; 4];
    r.read_exact(&mut b).map_err(|e| e.to_string())?;
    Ok(u32::from_le_bytes(b))
}
fn read_hash(r: &mut impl Read) -> Result<Hash64, String> {
    let mut b = [0; 64];
    r.read_exact(&mut b).map_err(|e| e.to_string())?;
    Ok(Hash64::from_bytes(b))
}
fn at(file: &mut File, offset: u64, length: usize) -> Result<Vec<u8>, String> {
    if length > 65_536 {
        return Err("public table disk read exceeds bounded part".into());
    }
    file.seek(SeekFrom::Start(offset)).map_err(|e| e.to_string())?;
    let mut out = vec![0; length];
    file.read_exact(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}
impl FileTableV1 {
    /// Import every original leaf with exact EOF before publishing this holding. Neither a
    /// pre-existing index nor a file's path/mtime supplies a root or an authentication claim.
    pub(crate) fn open_v1(path: &Path) -> Result<Arc<Self>, String> {
        let file = File::open(path).map_err(|e| e.to_string())?;
        let original_len = file.metadata().map_err(|e| e.to_string())?.len();
        let mut input = BufReader::with_capacity(65_536, file);
        let mut magic = [0; 8];
        input.read_exact(&mut magic).map_err(|e| e.to_string())?;
        if &magic != PALW_TOKEN_TABLE_FILE_MAGIC_V1 {
            return Err("not an original PALWTTB1 public table".into());
        }
        let vocab = read_u32(&mut input)?;
        if vocab == 0 || u64::from(vocab) >= PALW_TOKEN_TABLE_MAX_LEAVES_V1 {
            return Err("public table vocabulary exceeds original Merkle format".into());
        }
        let maximum = 12u64
            .checked_add(u64::from(vocab).checked_mul(u64::from(u32::MAX) + 5).ok_or("public table size overflow")?)
            .and_then(|n| n.checked_add(u64::from(vocab) * 4 + 68))
            .ok_or("public table size overflow")?;
        if original_len > maximum {
            return Err("public table bytes exceed original bounded format".into());
        }
        let scratch = host_ledger_v1()
            .reserve(
                PalwMemoryReservationKeyV1 { role: "public-table-index-build", class_id: Hash64::default(), job: Hash64::default() },
                u64::from(vocab).checked_mul(8).and_then(|n| n.checked_add(262_144)).ok_or("public table scratch overflow")?,
            )
            .map_err(|e| e.to_string())?;
        let mut name = path.as_os_str().to_owned();
        name.push(".public-index");
        let index_path = PathBuf::from(name);
        let parent = index_path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
        let temporary = tempfile::Builder::new().prefix(".palw-public-table-").tempfile_in(parent).map_err(|e| e.to_string())?;
        let mut index = temporary.reopen().map_err(|e| e.to_string())?;
        let mut offset = 12u64;
        let mut buffer = [0u8; 65_536];
        for id in 0..vocab {
            let mut present = [0; 1];
            input.read_exact(&mut present).map_err(|e| e.to_string())?;
            offset = offset.checked_add(1).ok_or("public table offset overflow")?;
            if present[0] > 1 {
                return Err("public table Option discriminant is not canonical".into());
            }
            let length = if present[0] == 1 {
                let n = read_u32(&mut input)?;
                offset += 4;
                n
            } else {
                0
            };
            let begin = offset;
            let mut leaf = blake2b_simd::Params::new().hash_length(64).key(PALW_TOKEN_TABLE_DOMAIN_LEAF_V1).to_state();
            leaf.update(&id.to_le_bytes());
            leaf.update(&present);
            if present[0] == 1 {
                leaf.update(&length.to_le_bytes());
            }
            let mut remaining = u64::from(length);
            while remaining > 0 {
                let n = remaining.min(buffer.len() as u64) as usize;
                input.read_exact(&mut buffer[..n]).map_err(|e| e.to_string())?;
                leaf.update(&buffer[..n]);
                remaining -= n as u64;
            }
            offset = offset.checked_add(u64::from(length)).ok_or("public table offset overflow")?;
            let hash = Hash64::from_bytes(leaf.finalize().as_bytes().try_into().expect("64"));
            index
                .write_all(&begin.to_le_bytes())
                .and_then(|_| index.write_all(&length.to_le_bytes()))
                .and_then(|_| index.write_all(&[present[0], 0, 0, 0]))
                .and_then(|_| index.write_all(hash.as_byte_slice()))
                .map_err(|e| e.to_string())?;
        }
        let eog_count = read_u32(&mut input)?;
        if eog_count == 0 || eog_count > vocab {
            return Err("public table EOG count exceeds vocabulary".into());
        }
        let mut eog = Vec::new();
        eog.try_reserve_exact(eog_count as usize).map_err(|_| "public EOG allocation refused")?;
        for _ in 0..eog_count {
            let id = read_u32(&mut input)?;
            if id >= vocab {
                return Err("public EOG ID exceeds vocabulary".into());
            }
            eog.push(id)
        }
        eog.sort_unstable();
        if eog.windows(2).any(|w| w[0] == w[1]) {
            return Err("public table EOG set contains duplicate IDs".into());
        }
        let tokenizer = read_hash(&mut input)?;
        if input.read(&mut buffer[..1]).map_err(|e| e.to_string())? != 0 {
            return Err("public table has trailing bytes".into());
        }
        let leaf = table_eog_leaf_hash_v1(&eog);
        index
            .write_all(&0u64.to_le_bytes())
            .and_then(|_| index.write_all(&0u32.to_le_bytes()))
            .and_then(|_| index.write_all(&[0; 4]))
            .and_then(|_| index.write_all(leaf.as_byte_slice()))
            .map_err(|e| e.to_string())?;
        let eog_offset = (u64::from(vocab) + 1).checked_mul(RECORD).ok_or("public index offset overflow")?;
        for id in eog {
            index.write_all(&id.to_le_bytes()).map_err(|e| e.to_string())?
        }
        let mut end = eog_offset + u64::from(eog_count) * 4;
        let count = u64::from(vocab) + 1;
        let mut levels = Vec::with_capacity(23);
        levels.push((end, count));
        // Canonical tree levels are sequential files, not a resident all-leaves buffer.
        for id in 0..count {
            let hash = Hash64::from_bytes(at(&mut index, id * RECORD + 16, 64)?.try_into().expect("64"));
            index.seek(SeekFrom::Start(end)).map_err(|e| e.to_string())?;
            index.write_all(step_merkle_leaf_v1(id, &hash).as_byte_slice()).map_err(|e| e.to_string())?;
            end += 64;
        }
        let mut level = 0usize;
        while levels[level].1 > 1 {
            let (begin, width) = levels[level];
            let next = end;
            for pair in 0..width.div_ceil(2) {
                let left = Hash64::from_bytes(at(&mut index, begin + pair * 128, 64)?.try_into().expect("64"));
                let hash = if pair * 2 + 1 < width {
                    let right = Hash64::from_bytes(at(&mut index, begin + pair * 128 + 64, 64)?.try_into().expect("64"));
                    step_merkle_node_v1(&left, &right)
                } else {
                    left
                };
                index.seek(SeekFrom::Start(end)).map_err(|e| e.to_string())?;
                index.write_all(hash.as_byte_slice()).map_err(|e| e.to_string())?;
                end += 64;
            }
            levels.push((next, width.div_ceil(2)));
            level += 1;
        }
        let root = Hash64::from_bytes(at(&mut index, levels.last().expect("one level").0, 64)?.try_into().expect("64"));
        index.sync_all().map_err(|e| e.to_string())?;
        temporary.persist(&index_path).map_err(|e| e.to_string())?;
        if let Some(parent) = index_path.parent().filter(|p| !p.as_os_str().is_empty()) {
            File::open(parent).and_then(|f| f.sync_all()).map_err(|e| e.to_string())?;
        }
        let holding = host_ledger_v1()
            .reserve(PalwMemoryReservationKeyV1 { role: "public-table-index-holding", class_id: tokenizer, job: root }, 131_072)
            .map_err(|e| e.to_string())?;
        drop(scratch);
        Ok(Arc::new(Self {
            original: Mutex::new(input.into_inner()),
            index: Mutex::new(index),
            vocab,
            tokenizer,
            root,
            eog_count,
            eog_offset,
            levels,
            _holding: holding,
        }))
    }
    fn record_v1(&self, id: u32) -> Result<(u64, Option<u32>), String> {
        if id >= self.vocab {
            return Err("public table rendering ID exceeds vocabulary".into());
        }
        let mut index = self.index.lock().map_err(|_| "public table index lock poisoned")?;
        let bytes = at(&mut index, u64::from(id) * RECORD, 16)?;
        let offset = u64::from_le_bytes(bytes[..8].try_into().expect("8"));
        let length = u32::from_le_bytes(bytes[8..12].try_into().expect("4"));
        if bytes[12] > 1 || bytes[13..] != [0; 3] {
            return Err("public table index metadata corrupted".into());
        }
        Ok((offset, (bytes[12] == 1).then_some(length)))
    }
}
impl PalwPublicTokenTableSourceV1 for FileTableV1 {
    fn root_v1(&self) -> Hash64 {
        self.root
    }
    fn tokenizer_id_v1(&self) -> Hash64 {
        self.tokenizer
    }
    fn vocab_v1(&self) -> u32 {
        self.vocab
    }
    fn eog_count_v1(&self) -> u32 {
        self.eog_count
    }
    fn eog_part_v1(&self, offset: u32, maximum: u32) -> Result<Vec<u32>, String> {
        if offset > self.eog_count || maximum > 16_384 {
            return Err("public EOG read exceeds exact bounded set".into());
        }
        let length = (self.eog_count - offset).min(maximum);
        let mut index = self.index.lock().map_err(|_| "public table index lock poisoned")?;
        Ok(at(&mut index, self.eog_offset + u64::from(offset) * 4, length as usize * 4)?
            .chunks_exact(4)
            .map(|b| u32::from_le_bytes(b.try_into().expect("4")))
            .collect())
    }
    fn entry_length_v1(&self, id: u32) -> Result<Option<u32>, String> {
        self.record_v1(id).map(|r| r.1)
    }
    fn read_part_v1(&self, id: u32, offset: u32, maximum: u32) -> Result<Vec<u8>, String> {
        if maximum > PALW_PUBLIC_TABLE_PART_BYTES_V1 {
            return Err("public table read exceeds bounded part".into());
        }
        let (begin, length) = self.record_v1(id)?;
        let length = length.unwrap_or(0);
        if offset > length {
            return Err("public rendering offset exceeds original length".into());
        }
        let mut original = self.original.lock().map_err(|_| "public table original file lock poisoned")?;
        at(
            &mut original,
            begin.checked_add(u64::from(offset)).ok_or("public table read offset overflow")?,
            (length - offset).min(maximum) as usize,
        )
    }
    fn opening_v1(&self, id: u32) -> Result<PalwStepOpeningV1, String> {
        if id > self.vocab {
            return Err("public table opening exceeds vocabulary/EOG leaf".into());
        }
        let mut index = self.index.lock().map_err(|_| "public table index lock poisoned")?;
        let leaf_hash = Hash64::from_bytes(at(&mut index, u64::from(id) * RECORD + 16, 64)?.try_into().expect("64"));
        let mut position = u64::from(id);
        let mut siblings = Vec::with_capacity(self.levels.len());
        for (begin, width) in self.levels.iter().take(self.levels.len() - 1) {
            if (position ^ 1) < *width {
                siblings.push(Hash64::from_bytes(at(&mut index, begin + (position ^ 1) * 64, 64)?.try_into().expect("64")));
            }
            position /= 2;
        }
        Ok(PalwStepOpeningV1 { leaf_index: u64::from(id), leaf_hash, siblings })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_fp_constraint_job_v1::{PalwTokenTableV1, palw_token_table_file_encode_v1};
    #[test]
    fn original_file_root_parts_and_promoted_merkle_paths_match_legacy_codec() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("table.palwtokens");
        let table = PalwTokenTableV1 {
            entries: vec![Some(b"a".to_vec()), None, Some(Vec::new()), Some(vec![b'x'; 131_079]), None],
            eog_token_ids: vec![4],
            tokenizer_id: Hash64::from_u64_word(8),
        };
        std::fs::write(&path, palw_token_table_file_encode_v1(&table)).unwrap();
        let source = FileTableV1::open_v1(&path).unwrap();
        assert_eq!(source.root_v1(), table.root());
        assert_eq!(source.eog_ids_v1().unwrap(), vec![4]);
        for id in 0..=source.vocab_v1() {
            assert_eq!(
                source.checked_opening_v1(id).unwrap(),
                kaspa_consensus_core::palw_public_token_table_v1::PalwPublicTokenTableSourceV1::opening_v1(&table, id).unwrap()
            );
        }
        assert_eq!(source.entry_length_v1(1).unwrap(), None);
        assert_eq!(source.entry_length_v1(2).unwrap(), Some(0));
        assert_eq!(source.read_part_v1(3, 65_536, 65_536).unwrap(), vec![b'x'; 65_536]);
        assert_eq!(source.read_part_v1(3, 131_072, 65_536).unwrap(), vec![b'x'; 7]);
        assert_eq!(source.rendering_digest_v1(3).unwrap(), table.rendering_digest_v1(3).unwrap());
        assert!(source.read_part_v1(3, 0, 65_537).is_err());
        drop(source);
        assert_eq!(FileTableV1::open_v1(&path).unwrap().root_v1(), table.root(), "restart rebuilds only from original EOF");
    }
    #[test]
    fn modified_public_file_or_index_cannot_supply_an_authenticated_rendering() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("table.palwtokens");
        let table = PalwTokenTableV1 {
            entries: vec![Some(b"a".to_vec()), None],
            eog_token_ids: vec![1],
            tokenizer_id: Hash64::from_u64_word(8),
        };
        std::fs::write(&path, palw_token_table_file_encode_v1(&table)).unwrap();
        let source = FileTableV1::open_v1(&path).unwrap();
        let mut file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
        file.seek(SeekFrom::Start(17)).unwrap();
        file.write_all(b"b").unwrap();
        assert!(source.rendering_digest_v1(0).is_err(), "modified bytes must pass original leaf EOF before use");
        let mut index = source.index.lock().unwrap();
        index.seek(SeekFrom::Start(16)).unwrap();
        index.write_all(Hash64::default().as_byte_slice()).unwrap();
        drop(index);
        assert!(source.checked_opening_v1(0).is_err(), "cached leaf metadata is not a credential");
    }
}
