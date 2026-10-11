//! Prospective Q36 prefix-cache byte codec. This names every state byte the engine resumes:
//! layer ascending, attention K then V position ascending, or GDN head matrices row-major
//! followed by oldest-first concatenated q/k/v convolution rows. Its staged encoder owns only
//! a bounded hash continuation; authenticity of each byte belongs to the typed outer court.
use crate::Hash64;
use crate::palw_rendering_stream_v1::LeafHashContinuationV1;
use crate::palw_step::{PalwLayerKindV1, PalwShapeProfileV3, PalwStepLaneV1};
use borsh::{BorshDeserialize, BorshSerialize};

pub const PALW_Q36_PREFIX_CACHE_DOMAIN_V1: &[u8] = b"misaka-palw/qwen36/prefix-cache/v1";
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwPrefixCacheUnitV1 {
    Key { layer: u16, position: u32 },
    Value { layer: u16, position: u32 },
    GdnHead { layer: u16, head: u32 },
    Convolution { layer: u16, age: u16 },
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwQwen36PrefixCachePlanV1 {
    class_id: Hash64,
    version: u16,
    layers: u16,
    interval: u16,
    prefix_tokens: u32,
    attention_values: u64,
    heads: u16,
    matrix_values: u64,
    conv_rows: u16,
    conv_values: u64,
    units: u64,
    total_bytes: u64,
}
impl PalwQwen36PrefixCachePlanV1 {
    pub fn new(profile: &PalwShapeProfileV3, prefix_tokens: u32) -> Result<Self, String> {
        profile.validate_shape().map_err(|e| e.to_string())?;
        if profile.lane != PalwStepLaneV1::Int32 || prefix_tokens == 0 || prefix_tokens > profile.n_ctx {
            return Err("the hybrid prefix needs a nonempty in-context integer cache".into());
        }
        let attention_values = u64::from(profile.attn_kv_heads)
            .checked_mul(u64::from(profile.attn_head_dim))
            .ok_or("the attention state width overflows")?;
        let matrix_values = u64::from(profile.gdn_head_k_dim)
            .checked_mul(u64::from(profile.gdn_head_v_dim))
            .ok_or("the GDN matrix width overflows")?;
        let conv_values = if profile.gdn_layer_exists() {
            let keys = crate::palw_step::palw_gdn_key_heads_v1(profile).map_err(|e| e.to_string())?;
            u64::from(keys)
                .checked_mul(u64::from(profile.gdn_head_k_dim))
                .and_then(|n| n.checked_mul(2))
                .and_then(|n| {
                    u64::from(profile.gdn_heads).checked_mul(u64::from(profile.gdn_head_v_dim)).and_then(|v| n.checked_add(v))
                })
                .ok_or("the convolution state width overflows")?
        } else {
            0
        };
        let mut units = 0u64;
        let mut values = 0u64;
        for layer in 0..profile.layer_count {
            let (count, words) = match profile.layer_kind(layer) {
                PalwLayerKindV1::Attention => {
                    if attention_values == 0 {
                        return Err("the declared attention cache is empty".into());
                    }
                    (
                        u64::from(prefix_tokens) * 2,
                        u64::from(prefix_tokens).checked_mul(2).and_then(|n| n.checked_mul(attention_values)),
                    )
                }
                PalwLayerKindV1::GatedDeltaNet => {
                    if profile.gdn_heads == 0 || matrix_values == 0 || profile.gdn_conv_kernel == 0 || conv_values == 0 {
                        return Err("the declared hybrid cache has an empty recurrence component".into());
                    }
                    (
                        u64::from(profile.gdn_heads) + u64::from(profile.gdn_conv_kernel),
                        u64::from(profile.gdn_heads)
                            .checked_mul(matrix_values)
                            .and_then(|n| u64::from(profile.gdn_conv_kernel).checked_mul(conv_values).and_then(|v| n.checked_add(v))),
                    )
                }
            };
            units = units.checked_add(count).ok_or("prefix units overflow")?;
            values = values.checked_add(words.ok_or("prefix component bytes overflow")?).ok_or("prefix values overflow")?;
        }
        let total_bytes = values.checked_mul(4).ok_or("prefix bytes overflow")?;
        Ok(Self {
            class_id: profile.shape_profile_id(),
            version: profile.version,
            layers: profile.layer_count,
            interval: profile.full_attention_interval,
            prefix_tokens,
            attention_values,
            heads: profile.gdn_heads,
            matrix_values,
            conv_rows: profile.gdn_conv_kernel,
            conv_values,
            units,
            total_bytes,
        })
    }
    pub fn unit_count(&self) -> u64 {
        self.units
    }
    pub fn total_bytes(&self) -> u64 {
        self.total_bytes
    }
    pub fn unit(&self, mut ordinal: u64) -> Option<(PalwPrefixCacheUnitV1, u64)> {
        if ordinal >= self.units {
            return None;
        }
        for layer in 0..self.layers {
            let attention = self.interval != 0
                && if self.version == crate::palw_step::PALW_STEP_OBJECT_VERSION_V2 {
                    u32::from(layer).is_multiple_of(u32::from(self.interval))
                } else {
                    (u32::from(layer) + 1).is_multiple_of(u32::from(self.interval))
                };
            let count = if attention { u64::from(self.prefix_tokens) * 2 } else { u64::from(self.heads) + u64::from(self.conv_rows) };
            if ordinal >= count {
                ordinal -= count;
                continue;
            }
            let (unit, words) = if attention {
                if ordinal < u64::from(self.prefix_tokens) {
                    (PalwPrefixCacheUnitV1::Key { layer, position: ordinal as u32 }, self.attention_values)
                } else {
                    (
                        PalwPrefixCacheUnitV1::Value { layer, position: (ordinal - u64::from(self.prefix_tokens)) as u32 },
                        self.attention_values,
                    )
                }
            } else if ordinal < u64::from(self.heads) {
                (PalwPrefixCacheUnitV1::GdnHead { layer, head: ordinal as u32 }, self.matrix_values)
            } else {
                (PalwPrefixCacheUnitV1::Convolution { layer, age: (ordinal - u64::from(self.heads)) as u16 }, self.conv_values)
            };
            return Some((unit, words.checked_mul(4)?));
        }
        None
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwQwen36PrefixCacheDigestV1 {
    plan: PalwQwen36PrefixCachePlanV1,
    ordinal: u64,
    offset: u64,
    hash: LeafHashContinuationV1,
}
impl PalwQwen36PrefixCacheDigestV1 {
    pub fn new(profile: &PalwShapeProfileV3, prefix_tokens: u32) -> Result<Self, String> {
        let plan = PalwQwen36PrefixCachePlanV1::new(profile, prefix_tokens)?;
        let mut hash = LeafHashContinuationV1::new(PALW_Q36_PREFIX_CACHE_DOMAIN_V1);
        hash.update(&1u16.to_le_bytes());
        hash.update(plan.class_id.as_byte_slice());
        hash.update(&prefix_tokens.to_le_bytes());
        Ok(Self { plan, ordinal: 0, offset: 0, hash })
    }
    pub fn next_unit(&self) -> Option<(PalwPrefixCacheUnitV1, u64, u64)> {
        self.plan.unit(self.ordinal).map(|(unit, length)| (unit, length, self.offset))
    }
    /// Codec only: the enclosing court authenticates public KV/projection openings or a
    /// completed genesis-zero head replay before feeding these bytes. Native backends feed
    /// their own actual cache through the same codec. No byte/result here proves guilt alone.
    pub fn push(&mut self, unit: PalwPrefixCacheUnitV1, offset: u64, bytes: &[u8]) -> Result<(), String> {
        let (expected, length) = self.plan.unit(self.ordinal).ok_or("the prefix cache digest is complete")?;
        if expected != unit
            || offset != self.offset
            || bytes.is_empty()
            || bytes.len() > 65_536
            || bytes.len() % 4 != 0
            || length.checked_sub(self.offset).is_none_or(|remaining| bytes.len() as u64 > remaining)
        {
            return Err("the prefix bytes do not fit the exact next canonical cache unit".into());
        }
        if self.offset == 0 {
            self.hash.update(&self.ordinal.to_le_bytes());
            self.hash.update(&length.to_le_bytes());
        }
        self.hash.update(bytes);
        self.offset += bytes.len() as u64;
        if self.offset == length {
            self.ordinal += 1;
            self.offset = 0;
        }
        Ok(())
    }
    pub fn finish(&self) -> Result<Hash64, String> {
        if self.ordinal != self.plan.units || self.offset != 0 {
            return Err("the prefix cache byte set is incomplete".into());
        }
        Ok(crate::palw_fp_prefix_v1::palw_fp_prefix_state_root_v1(&self.plan.class_id, self.plan.prefix_tokens, &self.hash.finish()))
    }
}
