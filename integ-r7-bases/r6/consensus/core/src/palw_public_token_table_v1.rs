//! Public table operands with bounded reads. Paths and cached digests never authenticate bytes.
use crate::{
    Hash64,
    palw_decode_constraint_v1::{PalwConstraintStateV1, PalwDecodeConstraintV1, constraint_step_v1},
    palw_fp_constraint_job_v1::{
        PALW_TOKEN_TABLE_DOMAIN_LEAF_V1, PALW_TOKEN_TABLE_MAX_LEAVES_V1, PalwTokenTableV1, table_eog_leaf_hash_v1, table_leaf_hash_v1,
    },
    palw_step_leg::{PalwStepOpeningV1, step_opening_root_capped_v1},
    palw_tokenizer_json_stream_v1::{PALW_TOKENIZER_RENDERING_DIGEST_DOMAIN_V1, PalwTokenizerRenderingDigestV1},
};
use std::{
    collections::BTreeMap,
    sync::{Arc, OnceLock, RwLock},
};

pub const PALW_PUBLIC_TABLE_PART_BYTES_V1: u32 = 65_536;

pub trait PalwPublicTokenTableSourceV1: Send + Sync {
    fn root_v1(&self) -> Hash64;
    fn tokenizer_id_v1(&self) -> Hash64;
    fn vocab_v1(&self) -> u32;
    fn eog_count_v1(&self) -> u32;
    fn eog_part_v1(&self, offset: u32, maximum: u32) -> Result<Vec<u32>, String>;
    /// None is an absent leaf; Some(0) is the original present-empty leaf.
    fn entry_length_v1(&self, id: u32) -> Result<Option<u32>, String>;
    fn read_part_v1(&self, id: u32, offset: u32, maximum: u32) -> Result<Vec<u8>, String>;
    fn opening_v1(&self, id: u32) -> Result<PalwStepOpeningV1, String>;

    fn checked_opening_v1(&self, id: u32) -> Result<PalwStepOpeningV1, String> {
        let opening = self.opening_v1(id)?;
        let count = u64::from(self.vocab_v1()) + 1;
        if opening.leaf_index != u64::from(id)
            || step_opening_root_capped_v1(count, &opening, PALW_TOKEN_TABLE_MAX_LEAVES_V1).map_err(|e| e.to_string())?
                != self.root_v1()
        {
            return Err("public table opening changes immutable original root".into());
        }
        Ok(opening)
    }
    fn eog_ids_v1(&self) -> Result<Vec<u32>, String> {
        if self.eog_count_v1() == 0 || self.eog_count_v1() > self.vocab_v1() {
            return Err("public table EOG geometry is not bounded by its vocabulary".into());
        }
        let mut ids = Vec::new();
        ids.try_reserve_exact(self.eog_count_v1() as usize).map_err(|_| "public EOG allocation refused")?;
        while ids.len() < self.eog_count_v1() as usize {
            let part = self.eog_part_v1(ids.len() as u32, 16_384)?;
            if part.is_empty() || part.len() > 16_384 || ids.len() + part.len() > self.eog_count_v1() as usize {
                return Err("public EOG part changes exact authenticated length".into());
            }
            ids.extend(part);
        }
        if ids.iter().any(|id| *id >= self.vocab_v1())
            || ids.windows(2).any(|w| w[0] >= w[1])
            || self.checked_opening_v1(self.vocab_v1())?.leaf_hash != table_eog_leaf_hash_v1(&ids)
        {
            return Err("public EOG bytes do not authenticate their original strict set".into());
        }
        Ok(ids)
    }
    /// Verify the ORIGINAL table leaf at EOF while observing a rendering in bounded parts.
    fn visit_rendering_v1(&self, id: u32, observer: &mut dyn FnMut(&[u8])) -> Result<Option<PalwTokenizerRenderingDigestV1>, String> {
        let opening = self.checked_opening_v1(id)?;
        let length = self.entry_length_v1(id)?;
        let mut leaf = blake2b_simd::Params::new().hash_length(64).key(PALW_TOKEN_TABLE_DOMAIN_LEAF_V1).to_state();
        leaf.update(&id.to_le_bytes());
        leaf.update(&[u8::from(length.is_some())]);
        let mut rendering = blake2b_simd::Params::new().hash_length(64).key(PALW_TOKENIZER_RENDERING_DIGEST_DOMAIN_V1).to_state();
        if let Some(length) = length {
            leaf.update(&length.to_le_bytes());
            let mut offset = 0u32;
            while offset < length {
                let maximum = (length - offset).min(PALW_PUBLIC_TABLE_PART_BYTES_V1);
                let bytes = self.read_part_v1(id, offset, maximum)?;
                if bytes.len() != maximum as usize {
                    return Err("public rendering read changes original exact part".into());
                }
                leaf.update(&bytes);
                rendering.update(&bytes);
                observer(&bytes);
                offset = offset.checked_add(maximum).ok_or("public rendering offset overflow")?;
            }
        }
        if Hash64::from_bytes(leaf.finalize().as_bytes().try_into().expect("64")) != opening.leaf_hash {
            return Err("public rendering bytes do not open original authenticated leaf".into());
        }
        Ok(length.map(|length| PalwTokenizerRenderingDigestV1 {
            length: u64::from(length),
            digest: Hash64::from_bytes(rendering.finalize().as_bytes().try_into().expect("64")),
        }))
    }
    fn rendering_digest_v1(&self, id: u32) -> Result<Option<PalwTokenizerRenderingDigestV1>, String> {
        self.visit_rendering_v1(id, &mut |_| {})
    }
    /// Observer state is tentative until original leaf EOF authenticates, then may be committed.
    fn admit_v1(
        &self,
        c: &PalwDecodeConstraintV1,
        state: &PalwConstraintStateV1,
        id: u32,
    ) -> Result<Option<PalwConstraintStateV1>, String> {
        let mut next = Some(state.clone());
        let present = self
            .visit_rendering_v1(id, &mut |bytes| {
                for &byte in bytes {
                    if let Some(s) = next.as_mut() {
                        if !constraint_step_v1(c, s, byte) {
                            next = None
                        }
                    }
                }
            })?
            .is_some();
        Ok(if present { next } else { None })
    }
}

impl PalwPublicTokenTableSourceV1 for PalwTokenTableV1 {
    fn root_v1(&self) -> Hash64 {
        self.root()
    }
    fn tokenizer_id_v1(&self) -> Hash64 {
        self.tokenizer_id
    }
    fn vocab_v1(&self) -> u32 {
        self.vocab()
    }
    fn eog_count_v1(&self) -> u32 {
        self.eog_token_ids.len() as u32
    }
    fn eog_part_v1(&self, offset: u32, maximum: u32) -> Result<Vec<u32>, String> {
        if maximum > 16_384 {
            return Err("public EOG part exceeds bounded carrier".into());
        }
        let mut ids = self.eog_token_ids.clone();
        ids.sort_unstable();
        let start = offset as usize;
        let rest = ids.get(start..).ok_or("public EOG offset exceeds original set")?;
        Ok(rest[..rest.len().min(maximum as usize)].to_vec())
    }
    fn entry_length_v1(&self, id: u32) -> Result<Option<u32>, String> {
        self.entries
            .get(id as usize)
            .ok_or("public table ID exceeds vocabulary")?
            .as_ref()
            .map(|bytes| u32::try_from(bytes.len()).map_err(|_| "public rendering exceeds original u32 format".into()))
            .transpose()
    }
    fn read_part_v1(&self, id: u32, offset: u32, maximum: u32) -> Result<Vec<u8>, String> {
        if maximum > PALW_PUBLIC_TABLE_PART_BYTES_V1 {
            return Err("public table read exceeds bounded part".into());
        }
        let bytes = self.entries.get(id as usize).ok_or("public table ID exceeds vocabulary")?.as_deref().unwrap_or_default();
        let rest = bytes.get(offset as usize..).ok_or("public rendering offset exceeds original bytes")?;
        Ok(rest[..rest.len().min(maximum as usize)].to_vec())
    }
    fn opening_v1(&self, id: u32) -> Result<PalwStepOpeningV1, String> {
        crate::palw_step_leg::step_opening_from_leaf_source_capped_v1(
            u64::from(self.vocab()) + 1,
            u64::from(id),
            PALW_TOKEN_TABLE_MAX_LEAVES_V1,
            |i| {
                if i == u64::from(self.vocab()) {
                    table_eog_leaf_hash_v1(&self.eog_token_ids)
                } else {
                    table_leaf_hash_v1(i as u32, self.entries[i as usize].as_deref())
                }
            },
        )
        .map_err(|e| e.to_string())
    }
}

type SourceMapV1 = BTreeMap<(Hash64, Hash64), Arc<dyn PalwPublicTokenTableSourceV1>>;
static SOURCES: OnceLock<RwLock<SourceMapV1>> = OnceLock::new();
pub fn register_public_token_table_v1(source: Arc<dyn PalwPublicTokenTableSourceV1>) -> Result<Hash64, String> {
    if source.vocab_v1() == 0 || u64::from(source.vocab_v1()) >= PALW_TOKEN_TABLE_MAX_LEAVES_V1 {
        return Err("public table vocabulary exceeds original Merkle format".into());
    }
    let key = (source.root_v1(), source.tokenizer_id_v1());
    SOURCES.get_or_init(Default::default).write().map_err(|_| "public table registry poisoned")?.insert(key, source);
    Ok(key.0)
}
pub fn public_token_table_v1(root: Hash64, tokenizer: Hash64) -> Option<Arc<dyn PalwPublicTokenTableSourceV1>> {
    SOURCES.get_or_init(Default::default).read().ok()?.get(&(root, tokenizer)).cloned().or_else(|| {
        crate::palw_fp_constraint_job_v1::palw_token_tables_v1()
            .for_root_and_tokenizer(&root, &tokenizer)
            .map(|table| table as Arc<dyn PalwPublicTokenTableSourceV1>)
    })
}
