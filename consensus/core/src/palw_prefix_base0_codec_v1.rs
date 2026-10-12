//! Prospective BASE0 i8 cache codec. Public step rows are i32 narrowed code values; every
//! lane must fit i8 before entering this byte digest. A malformed narrowing remains an
//! arithmetic court fault, never a guessed state root. The outer staged court owns/authenticates
//! the claim, canonical tiles and every transition before using this codec's completed root.
use crate::Hash64;
use crate::palw_prefix_state_court_v1::PalwPrefixKvPlanV1;
use crate::palw_rendering_stream_v1::LeafHashContinuationV1;
use crate::palw_step::{PalwShapeProfileV3, PalwStepCoordinateV1};
use borsh::{BorshDeserialize, BorshSerialize};
pub const PALW_BASE0_PREFIX_I8_DIGEST_DOMAIN_V1: &[u8] = b"misaka-palw/base0/prefix-i8-kv-digest/v1";
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwBase0PrefixDigestV1 {
    class_id: Hash64,
    prefix_tokens: u32,
    ordinal: u64,
    offset_values: u32,
    values_written: u64,
    hash: LeafHashContinuationV1,
}
impl PalwBase0PrefixDigestV1 {
    pub fn new(profile: &PalwShapeProfileV3, prefix_tokens: u32) -> Result<Self, String> {
        if prefix_tokens == 0 || prefix_tokens > profile.n_ctx {
            return Err("the i8 prefix is outside its registered context".into());
        }
        crate::palw_class_semantics_v1::PalwPrefixStateRelationV1::Base0Int8KvV1.check_profile(profile)?;
        PalwPrefixKvPlanV1::of_profile(profile, prefix_tokens)?;
        let mut hash = LeafHashContinuationV1::new(PALW_BASE0_PREFIX_I8_DIGEST_DOMAIN_V1);
        hash.update(&u64::from(profile.layer_count).to_le_bytes());
        Ok(Self { class_id: profile.shape_profile_id(), prefix_tokens, ordinal: 0, offset_values: 0, values_written: 0, hash })
    }
    fn plan(&self, profile: &PalwShapeProfileV3) -> Result<PalwPrefixKvPlanV1, String> {
        if profile.shape_profile_id() != self.class_id || self.hash.byte_count() != Some(128 + 8 + u128::from(self.values_written)) {
            return Err("the i8 prefix continuation changes class or its authenticated byte count".into());
        }
        let plan = PalwPrefixKvPlanV1::of_profile(profile, self.prefix_tokens)?;
        if self.ordinal > plan.tile_count {
            return Err("the i8 prefix tile ordinal exceeds its canonical walk".into());
        }
        Ok(plan)
    }
    pub fn next_tile_ordinal_v1(&self) -> u64 {
        self.ordinal
    }
    pub fn next_coordinate(&self, profile: &PalwShapeProfileV3) -> Result<Option<(PalwStepCoordinateV1, u32, u32)>, String> {
        let plan = self.plan(profile)?;
        Ok(plan.coordinate(profile, self.ordinal).map(|(coord, count)| (coord, count, self.offset_values)))
    }
    /// The enclosing court first authenticates these exact i32 lanes against the claim's old
    /// tile leaf hash, including offset and canonical coordinate. This byte converter has no
    /// authority to admit a supplied row/root. Invalid chunks do not mutate owned state.
    pub fn push_values(
        &mut self,
        profile: &PalwShapeProfileV3,
        coordinate: PalwStepCoordinateV1,
        offset_values: u32,
        values_i32_le: &[u8],
    ) -> Result<(), String> {
        let plan = self.plan(profile)?;
        let (expected, count) = plan.coordinate(profile, self.ordinal).ok_or("the i8 prefix is complete")?;
        if coordinate != expected
            || offset_values != self.offset_values
            || values_i32_le.is_empty()
            || values_i32_le.len() > 65_536
            || !values_i32_le.len().is_multiple_of(4)
        {
            return Err("the i8 prefix chunk changes its next coordinate or bounded aligned offset".into());
        }
        let words = u32::try_from(values_i32_le.len() / 4).map_err(|_| "i8 prefix chunk count overflows")?;
        let end =
            offset_values.checked_add(words).filter(|end| *end <= count).ok_or("the i8 prefix chunk exceeds its canonical tile")?;
        let mut narrowed = Vec::with_capacity(words as usize);
        for word in values_i32_le.chunks_exact(4) {
            let value = i32::from_le_bytes(word.try_into().expect("a complete four-byte lane"));
            narrowed
                .push(i8::try_from(value).map_err(|_| "an authenticated BASE0 cache-write lane violates its narrowing kernel")? as u8);
        }
        let written = self.values_written.checked_add(u64::from(words)).ok_or("i8 prefix byte count overflows")?;
        self.hash.update(&narrowed);
        self.values_written = written;
        self.offset_values = end;
        if end == count {
            self.ordinal += 1;
            self.offset_values = 0;
        }
        Ok(())
    }
    pub fn finish(&self, profile: &PalwShapeProfileV3) -> Result<Hash64, String> {
        let plan = self.plan(profile)?;
        let expected = u64::from(profile.layer_count)
            .checked_mul(2)
            .and_then(|n| n.checked_mul(u64::from(self.prefix_tokens)))
            .and_then(|n| n.checked_mul(u64::from(plan.width)))
            .ok_or("the i8 prefix byte set overflows")?;
        if self.ordinal != plan.tile_count || self.offset_values != 0 || self.values_written != expected {
            return Err("the i8 prefix cache set is incomplete".into());
        }
        Ok(crate::palw_fp_prefix_v1::palw_fp_prefix_state_root_v1(&self.class_id, self.prefix_tokens, &self.hash.finish()))
    }
}
