//! Exact legacy recurrence enumeration for the versioned RFC06 state-anchor
//! consumer. Old map IDs, old root construction and old proof dispatch are not
//! reinterpreted here. This is data layout/authentication, not a computation
//! validity argument: a court still deterministically recomputes the step.
use crate::palw_state_chunk_map as maps;
use crate::palw_step::{PalwLayerKindV1, PalwShapeProfileV3};
use crate::palw_step_leg::{
    PALW_STEP_LEG_MAX_STATE_CHUNK_BYTES, PALW_STEP_LEG_MAX_STATE_CHUNKS, PALW_STEP_LEG_MAX_STATE_DEPTH_V4, state_v4_path_len,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwGdnStateLayoutV2 {
    FullConvV1,
    HeadConvV2,
    KeyHeadConvV3,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwGdnStateChunkKindV2 {
    Delta,
    Conv,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwGdnStateChunkEntryV2 {
    pub kind: PalwGdnStateChunkKindV2,
    pub layer: u16,
    /// A delta head, a gathered convolution head, or zero for v1's whole window.
    pub head: u32,
    pub row_start: u32,
    pub row_count: u32,
    pub row_bytes: u32,
}
impl PalwGdnStateChunkEntryV2 {
    pub fn byte_len(self) -> u64 {
        u64::from(self.row_count) * u64::from(self.row_bytes)
    }
}

/// Borrowed geometry: deriving an input bound must not allocate a layer Vec.
/// Layer order is the signed profile's actual recurrence-layer order.
pub struct PalwGdnStateGeometryV2<'a> {
    profile: &'a PalwShapeProfileV3,
    pub layout: PalwGdnStateLayoutV2,
    pub heads: u32,
    pub key_heads: u32,
    pub k_dim: u32,
    pub v_dim: u32,
    pub conv_kernel: u32,
    pub conv_width: u32,
    pub delta_row_bytes: u32,
    pub conv_row_bytes: u32,
    pub delta_rows_per_chunk: u32,
    pub conv_rows_per_chunk: u32,
    recurrence_layers: u64,
    chunks: u64,
}
fn mul(a: u64, b: u64) -> Result<u64, &'static str> {
    a.checked_mul(b).ok_or("recurrence layout multiplication overflows")
}
fn add(a: u64, b: u64) -> Result<u64, &'static str> {
    a.checked_add(b).ok_or("recurrence layout sum overflows")
}
impl<'a> PalwGdnStateGeometryV2<'a> {
    pub fn of(profile: &'a PalwShapeProfileV3) -> Result<Self, &'static str> {
        let id = profile.state_chunk_map_id;
        let layout = if id == maps::gdn_state_chunk_map_id_v1() || id == maps::hybrid_state_chunk_map_id_v1() {
            PalwGdnStateLayoutV2::FullConvV1
        } else if id == maps::gdn_state_chunk_map_id_v2()
            || id == maps::hybrid_state_chunk_map_id_v2()
            || id == maps::hybrid_state_chunk_map_id_v3()
            || id == maps::hybrid_state_chunk_map_id_v4()
        {
            PalwGdnStateLayoutV2::HeadConvV2
        } else if id == maps::gdn_state_chunk_map_id_v3() || maps::palw_profile_spells_key_heads_v1(profile) {
            PalwGdnStateLayoutV2::KeyHeadConvV3
        } else {
            return Err("class map declares no supported recurrence layout");
        };
        let heads = u32::from(profile.gdn_heads);
        let k_dim = u32::from(profile.gdn_head_k_dim);
        let v_dim = u32::from(profile.gdn_head_v_dim);
        let conv_kernel = u32::from(profile.gdn_conv_kernel);
        if heads == 0 || k_dim == 0 || v_dim == 0 || conv_kernel == 0 {
            return Err("zero recurrence geometry");
        }
        let key_heads = if layout == PalwGdnStateLayoutV2::KeyHeadConvV3 {
            if profile.version != crate::palw_step::PALW_STEP_OBJECT_VERSION_V3 {
                return Err("key-head recurrence map requires its signed version-3 profile");
            }
            crate::palw_step::palw_gdn_key_heads_v1(profile)
                .map_err(|_| "key-head map has no authoritative graph-derived key heads")?
        } else {
            heads
        };
        if key_heads == 0 || !heads.is_multiple_of(key_heads) {
            return Err("key heads do not group value heads");
        }
        let recurrence_layers =
            (0..profile.layer_count).filter(|layer| profile.layer_kind(*layer) == PalwLayerKindV1::GatedDeltaNet).count() as u64;
        if recurrence_layers == 0 {
            return Err("map has no signed recurrence layers");
        }
        let conv_width = add(mul(mul(2, u64::from(key_heads))?, u64::from(k_dim))?, mul(u64::from(heads), u64::from(v_dim))?)?;
        let delta_row_bytes = mul(u64::from(k_dim), 4)?;
        let conv_row_bytes = if layout == PalwGdnStateLayoutV2::FullConvV1 {
            mul(conv_width, 4)?
        } else {
            mul(add(mul(2, u64::from(k_dim))?, u64::from(v_dim))?, 4)?
        };
        let cap = PALW_STEP_LEG_MAX_STATE_CHUNK_BYTES as u64;
        if delta_row_bytes > cap || conv_row_bytes > cap || conv_width > u64::from(u32::MAX) {
            return Err("recurrence row exceeds the existing chunk geometry cap");
        }
        let delta_rows = (cap / delta_row_bytes).min(u64::from(v_dim)) as u32;
        let conv_rows = (cap / conv_row_bytes).min(u64::from(conv_kernel)) as u32;
        // These composed maps promise one block per head, unlike the older solo
        // maps which may split a head over several blocks. Reusing the solo
        // enumeration without this check would authenticate different indices.
        let composed_heads = id == maps::hybrid_state_chunk_map_id_v3() || maps::palw_profile_is_held_v4(profile);
        if composed_heads && (delta_rows != v_dim || conv_rows != conv_kernel) {
            return Err("composed recurrence map requires one complete chunk per head");
        }
        let delta = mul(mul(recurrence_layers, u64::from(heads))?, u64::from(v_dim.div_ceil(delta_rows)))?;
        let conv_heads = if layout == PalwGdnStateLayoutV2::FullConvV1 { 1 } else { u64::from(heads) };
        let conv = mul(mul(recurrence_layers, conv_heads)?, u64::from(conv_kernel.div_ceil(conv_rows)))?;
        let chunks = add(delta, conv)?;
        if maps::palw_profile_is_held_v4(profile) {
            // A held recurrence slice is (kind,layer), its blocks are heads.
            // Its bound is path depth, never the flat legacy chunk-count wall.
            // Match the conservative top-slice envelope used by the held KV
            // geometry; that geometry separately checks the attention blocks.
            let attention_layers =
                (0..profile.layer_count).filter(|layer| profile.layer_kind(*layer) == PalwLayerKindV1::Attention).count() as u64;
            let slices = add(mul(2, attention_layers)?, mul(2, u64::from(profile.layer_count))?)?;
            let depth = state_v4_path_len(u64::from(heads), 0) + state_v4_path_len(slices, 0);
            if depth > PALW_STEP_LEG_MAX_STATE_DEPTH_V4 {
                return Err("recurrence opening exceeds the existing held depth cap");
            }
        } else if chunks > PALW_STEP_LEG_MAX_STATE_CHUNKS as u64 {
            return Err("recurrence chunk count exceeds existing flat-leg cap");
        }
        Ok(Self {
            profile,
            layout,
            heads,
            key_heads,
            k_dim,
            v_dim,
            conv_kernel,
            conv_width: conv_width as u32,
            delta_row_bytes: delta_row_bytes as u32,
            conv_row_bytes: conv_row_bytes as u32,
            delta_rows_per_chunk: delta_rows,
            conv_rows_per_chunk: conv_rows,
            recurrence_layers,
            chunks,
        })
    }
    pub fn chunk_count(&self) -> u64 {
        self.chunks
    }
    pub fn layers(&self) -> impl Iterator<Item = u16> + '_ {
        (0..self.profile.layer_count).filter(|layer| self.profile.layer_kind(*layer) == PalwLayerKindV1::GatedDeltaNet)
    }
    /// Locate a delta block without walking unrelated heads, conv state or the
    /// full checkpoint. This is also the exact selected-layer membership order.
    pub fn delta_chunk_index(&self, layer: u16, head: u32, block: u32) -> Option<u64> {
        if head >= self.heads {
            return None;
        }
        let per_head = self.v_dim.div_ceil(self.delta_rows_per_chunk);
        if block >= per_head {
            return None;
        }
        let ordinal = self.layers().position(|candidate| candidate == layer)? as u64;
        Some((ordinal * u64::from(self.heads) + u64::from(head)) * u64::from(per_head) + u64::from(block))
    }
    pub fn entry(&self, index: u64) -> Option<PalwGdnStateChunkEntryV2> {
        if index >= self.chunks {
            return None;
        }
        let delta_per_head = u64::from(self.v_dim.div_ceil(self.delta_rows_per_chunk));
        let delta_total = self.recurrence_layers * u64::from(self.heads) * delta_per_head;
        let (kind, within, heads, per_head, rows, row_cap, row_bytes) = if index < delta_total {
            (
                PalwGdnStateChunkKindV2::Delta,
                index,
                u64::from(self.heads),
                delta_per_head,
                self.v_dim,
                self.delta_rows_per_chunk,
                self.delta_row_bytes,
            )
        } else {
            (
                PalwGdnStateChunkKindV2::Conv,
                index - delta_total,
                if self.layout == PalwGdnStateLayoutV2::FullConvV1 { 1 } else { u64::from(self.heads) },
                u64::from(self.conv_kernel.div_ceil(self.conv_rows_per_chunk)),
                self.conv_kernel,
                self.conv_rows_per_chunk,
                self.conv_row_bytes,
            )
        };
        let layer = self.layers().nth((within / (heads * per_head)) as usize)?;
        let within_layer = within % (heads * per_head);
        let head = (within_layer / per_head) as u32;
        let row_start = (within_layer % per_head) as u32 * row_cap;
        Some(PalwGdnStateChunkEntryV2 { kind, layer, head, row_start, row_count: (rows - row_start).min(row_cap), row_bytes })
    }
    /// Channel named by a per-head conv slice, in the engine's `[q|k|v]`
    /// region-major full window. This fixes both equal and grouped key heads.
    pub fn conv_channel(&self, head: u32, lane: u32) -> Option<u32> {
        if head >= self.heads || lane >= 2 * self.k_dim + self.v_dim {
            return None;
        }
        let key = head % self.key_heads;
        if lane < self.k_dim {
            Some(key * self.k_dim + lane)
        } else if lane < 2 * self.k_dim {
            Some(self.key_heads * self.k_dim + key * self.k_dim + lane - self.k_dim)
        } else {
            Some(2 * self.key_heads * self.k_dim + head * self.v_dim + lane - 2 * self.k_dim)
        }
    }
}
