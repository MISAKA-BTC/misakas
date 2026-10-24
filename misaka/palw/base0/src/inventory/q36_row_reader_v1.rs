//! Q36 row emission with a scoped, pre-admitted reusable byte buffer.
use super::{InventoryBuildError, Q36PlannedTensorV1, Q36RowsV1, Qwen36RowSinkV1, q36_missing, q36_view};
use kaspa_consensus_core::palw_native_resource_v1::{NativeCoverageV1, NativeDemandLeaseV1, reserve_native_requests_v1};
use std::{
    mem::size_of,
    ops::{Deref, DerefMut},
};

struct ReadBuffer {
    bytes: Vec<u8>,
    buffer_ticket: Option<NativeDemandLeaseV1>,
    _base_ticket: Option<NativeDemandLeaseV1>,
}
impl ReadBuffer {
    fn base_request() -> u64 {
        size_of::<Self>() as u64 * 2
    }
    fn new() -> Result<Self, NativeCoverageV1> {
        let ticket = reserve_native_requests_v1(Self::base_request())?;
        Ok(Self { bytes: Vec::new(), buffer_ticket: None, _base_ticket: ticket })
    }
    fn zeroed(&mut self, len: usize) -> Result<(), NativeCoverageV1> {
        if len > self.bytes.capacity() {
            let request = u64::try_from(len)
                .ok()
                .and_then(|n| n.checked_mul(2))
                .ok_or_else(|| NativeCoverageV1::overflow::<Self>("Q36 read buffer request"))?;
            // Keep the old ticket through reserve/realloc. A rejected growth
            // changes neither the old bytes nor their live reservation.
            let ticket = reserve_native_requests_v1(request)?;
            self.bytes
                .try_reserve_exact(len - self.bytes.len())
                .map_err(|e| NativeCoverageV1::unsupported::<Self>("Q36 read buffer allocation", &e.to_string()))?;
            self.buffer_ticket = ticket;
        }
        self.bytes.clear();
        self.bytes.resize(len, 0);
        Ok(())
    }
}
impl Deref for ReadBuffer {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        &self.bytes
    }
}
impl DerefMut for ReadBuffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.bytes
    }
}

/// **The emitter's only state: one scratch buffer and the size of one read** (ADR-0106). Every
/// row it hands over is a slice of `buf` (or of a planned row), so what the emitter holds is one
/// read block or one row — never a tensor, whatever the artifact's size.
pub(super) struct Q36RowReaderV1<'a> {
    artifact: &'a crate::qwen36::Qwen36ArtifactV1,
    buf: ReadBuffer,
    block: usize,
}

impl<'a> Q36RowReaderV1<'a> {
    pub(super) fn new(artifact: &'a crate::qwen36::Qwen36ArtifactV1, block: usize) -> Result<Self, InventoryBuildError> {
        Ok(Self { artifact, buf: ReadBuffer::new()?, block: block.max(1) })
    }
    /// Every row of one planned tensor, in ascending offset, into `sink`.
    pub(super) fn stream(&mut self, planned: &Q36PlannedTensorV1, sink: &mut Qwen36RowSinkV1<'_>) -> Result<(), InventoryBuildError> {
        let (name, layer) = (planned.name.as_str(), planned.layer);
        match &planned.rows {
            Q36RowsV1::Stored { store, total, stride } => {
                // Read a block of whole rows at a time: every block starts on a row, so the rows are
                // exactly a slicing of the tensor whatever the block.
                let stride = (*stride).max(1);
                let per_block = (self.block / stride).max(1) * stride;
                let mut at = 0usize;
                while at < *total {
                    let take = per_block.min(total - at);
                    self.buf.zeroed(take)?;
                    self.artifact.read_tensor_range_into(store, at, &mut self.buf).map_err(|e| q36_missing(&e))?;
                    let mut offset = 0usize;
                    while offset < take {
                        let end = (offset + stride).min(take);
                        sink(name, layer, (at + offset) as u32, &self.buf[offset..end])?;
                        offset = end;
                    }
                    at += take;
                }
            }
            Q36RowsV1::Zeros { total, stride } => {
                let stride = (*stride).max(1);
                self.buf.zeroed(stride.min(*total))?;
                let mut at = 0usize;
                while at < *total {
                    let end = (at + stride).min(*total);
                    sink(name, layer, at as u32, &self.buf[..end - at])?;
                    at = end;
                }
            }
            Q36RowsV1::Params { stored, rule, chunk } => {
                // Row `k` is lanes `[k·chunk, (k+1)·chunk)` at byte `k·chunk·17`, the chunking the
                // arms read; each row's wire bytes are written as it is emitted.
                let w = kaspa_consensus_core::palw_base0_a16::A16QuantParams::WIRE_BYTES;
                let view = q36_view(self.artifact.inventory_param_rows_v1(stored)?, *rule, stored)?;
                let chunk = (*chunk).max(1);
                let mut lane = 0usize;
                while lane < view.width {
                    let end = (lane + chunk).min(view.width);
                    let bytes = (end - lane).checked_mul(w).ok_or_else(|| NativeCoverageV1::overflow::<Self>("Q36 parameter row"))?;
                    self.buf.zeroed(bytes)?;
                    for l in lane..end {
                        self.buf[(l - lane) * w..(l - lane + 1) * w].copy_from_slice(&view.get(l).to_wire());
                    }
                    sink(name, layer, (lane * w) as u32, &self.buf)?;
                    lane = end;
                }
            }
            Q36RowsV1::Small(rows) => {
                for (start, bytes) in rows {
                    sink(name, layer, *start, bytes)?;
                }
            }
            Q36RowsV1::Rope { pairs, table_pairs } => {
                // One row per position: `cos` then `sin` at the class's ROTARY width — the slice of
                // the pinned table the partial rotation reads.
                let rope = &self.artifact.rope;
                let mut offset = 0u32;
                for position in 0..self.artifact.shape.max_position {
                    let start = position * table_pairs;
                    let bytes = pairs.checked_mul(8).ok_or_else(|| NativeCoverageV1::overflow::<Self>("Q36 rotary row"))?;
                    self.buf.zeroed(bytes)?;
                    for (i, v) in rope.cos_q[start..start + pairs].iter().chain(&rope.sin_q[start..start + pairs]).enumerate() {
                        self.buf[i * 4..(i + 1) * 4].copy_from_slice(&v.to_le_bytes());
                    }
                    sink(name, layer, offset, &self.buf)?;
                    offset = offset
                        .checked_add(
                            u32::try_from(self.buf.len()).map_err(|_| NativeCoverageV1::overflow::<Self>("Q36 rotary row length"))?,
                        )
                        .ok_or_else(|| NativeCoverageV1::overflow::<Self>("Q36 rotary row offset"))?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_qwen36_profile::{
        PalwQwen36GeometryV1, qwen36_profile_v5, qwen36_profile_v6, qwen36_profile_v7, qwen36_profile_v8,
    };

    fn geometry() -> PalwQwen36GeometryV1 {
        PalwQwen36GeometryV1 {
            layer_count: 4,
            full_attention_interval: 4,
            hidden_dim: 32,
            attn_heads: 4,
            attn_kv_heads: 2,
            attn_head_dim: 16,
            rope_dims: 4,
            rope_freq_base_bits: 0x4B18_9680,
            gdn_k_heads: 4,
            gdn_v_heads: 4,
            gdn_head_dim: 8,
            gdn_conv_kernel: 4,
            n_experts: 8,
            experts_per_token: 4,
            moe_dim: 16,
            shared_dim: 16,
            attn_output_gate: 1,
            vocab_size: 64,
            n_ctx: 8,
            n_threads: 1,
            rms_eps_q: 1,
            tile_len: 512,
        }
    }

    #[test]
    fn row_buffer_capacity_child() {
        use kaspa_consensus_core::palw_native_resource_v1::{NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1};
        let Ok(case) = std::env::var("MISAKA_Q36_ROW_BUFFER_CAPACITY_CASE") else {
            return;
        };
        let artifact = crate::qwen36::qwen36_dev_fixture_heads(4, 8, 4, 4, 8, 32);
        let base = ReadBuffer::base_request();
        let memory = match case.as_str() {
            "zero" => 0,
            "below" => base + 23,
            "row-refusal" => base,
            _ => base + 24,
        };
        arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: memory, database_bytes: 0 }));
        if case == "zero" {
            assert!(ReadBuffer::new().is_err());
        } else if case == "row-refusal" {
            let mut reader = Q36RowReaderV1::new(&artifact, 8).unwrap();
            let planned = Q36PlannedTensorV1 {
                name: "embedding".into(),
                layer: None,
                rows: Q36RowsV1::Stored { store: "token_embd.weight".into(), total: 8, stride: 4 },
            };
            let mut called = 0;
            let result = reader.stream(&planned, &mut |_, _, _, _| {
                called += 1;
                Ok(())
            });
            assert!(matches!(result, Err(InventoryBuildError::Resource(_))));
            assert_eq!(called, 0, "the unadmitted read publishes no row");
            drop(reader);
        } else {
            let mut buf = ReadBuffer::new().unwrap();
            buf.zeroed(4).unwrap();
            buf.copy_from_slice(&[1, 2, 3, 4]);
            if case == "below" {
                assert!(buf.zeroed(8).is_err());
                assert_eq!(&*buf, &[1, 2, 3, 4], "refusal preserves the old buffer");
                assert!(reserve_native_requests_v1(memory - base - 8 + 1).is_err());
            } else {
                let blocker = if case == "retry" { reserve_native_requests_v1(1).unwrap() } else { None };
                if case == "retry" {
                    assert!(buf.zeroed(8).is_err());
                    assert_eq!(&*buf, &[1, 2, 3, 4]);
                    drop(blocker);
                }
                buf.zeroed(8).unwrap();
                assert_eq!(&*buf, &[0; 8]);
                assert!(reserve_native_requests_v1(9).is_err(), "resident buffer keeps its ticket");
                assert!(buf.zeroed(usize::MAX).is_err());
                assert_eq!(&*buf, &[0; 8]);
            }
            drop(buf);
        }
        assert!(reserve_native_requests_v1(memory).is_ok(), "drop/refusal returns every read-buffer ticket");
    }

    #[test]
    fn old_new_peak_refusal_retry_and_lifetime_are_enforced() {
        for case in ["zero", "below", "exact", "retry", "row-refusal"] {
            let result = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "inventory::q36_row_reader_v1::tests::row_buffer_capacity_child", "--nocapture"])
                .env("MISAKA_Q36_ROW_BUFFER_CAPACITY_CASE", case)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{case}: {} {}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            );
            assert!(String::from_utf8_lossy(&result.stdout).contains("1 passed"), "child was not selected");
        }
    }

    #[test]
    fn graph_v5_to_v8_keep_the_prechange_inventory_roots_at_every_read_size() {
        use kaspa_consensus_core::palw_artifact::{PalwArtifactInventoryV1, PalwArtifactOperandV1};
        let artifact = crate::qwen36::qwen36_dev_fixture_heads(4, 8, 4, 4, 8, 32);
        let path = std::env::temp_dir().join(format!("misaka-inventory-roots-{}.palwq36", std::process::id()));
        let plan: Vec<_> = artifact.tensor_names().into_iter().map(|n| (n.to_string(), artifact.tensor(n).unwrap().len())).collect();
        let mut writer =
            crate::qwen36::Qwen36Writer::create(&path, &artifact.shape, &artifact.rope, artifact.params_map(), plan.clone()).unwrap();
        for (name, _) in &plan {
            writer.push(name, &artifact.tensor(name).unwrap()).unwrap();
        }
        writer.finish().unwrap();
        let mapped = crate::qwen36::open_artifact_with_residency(
            &path,
            crate::qwen36::Qwen36ResidencyPolicyV1::Bytes(artifact.weight_bytes() as u64),
        )
        .unwrap();
        let before = mapped.residency_stats().unwrap();
        let profiles = [
            qwen36_profile_v5(geometry()),
            qwen36_profile_v6(geometry()),
            qwen36_profile_v7(geometry()),
            qwen36_profile_v8(geometry()),
        ];
        // Recorded from the parent implementation, not from the new reader.
        let roots = [
            "51840a8a278d843532ba880c1c4d0cd6fdeabe4007cfea233f5e62270325dcf08bd2ea7208fbd45eece23ec300d27b4332dae7adcb1d842a709e7599dbbd4657",
            "f21462cf3f47dcabf2e5ffdecf5de131c93af2b9c3c610e6b3265371382eb252a139297f0d93228b39ad1383d50f06d1781b4490d95e341a62685ae8a3d8a1ae",
            "f21462cf3f47dcabf2e5ffdecf5de131c93af2b9c3c610e6b3265371382eb252a139297f0d93228b39ad1383d50f06d1781b4490d95e341a62685ae8a3d8a1ae",
            "f21462cf3f47dcabf2e5ffdecf5de131c93af2b9c3c610e6b3265371382eb252a139297f0d93228b39ad1383d50f06d1781b4490d95e341a62685ae8a3d8a1ae",
        ];
        for (index, profile) in profiles.into_iter().enumerate() {
            let profile = profile.unwrap();
            for source in [&artifact, &mapped] {
                for block in [1, 7, 17, 4096] {
                    let mut rows = Vec::new();
                    super::super::qwen36_visit_inventory_rows_in_blocks_v1(
                        source,
                        &profile,
                        block,
                        &mut |name, layer, start, bytes| {
                            rows.push(PalwArtifactOperandV1 {
                                tensor_name: name.into(),
                                layer,
                                row_start: start,
                                bytes: bytes.to_vec(),
                            });
                            Ok(())
                        },
                    )
                    .unwrap();
                    let inventory = PalwArtifactInventoryV1::new(rows).unwrap();
                    assert_eq!(inventory.root().to_string(), roots[index], "graph-v{} block {block}", index + 5);
                }
            }
        }
        let after = mapped.residency_stats().unwrap();
        assert_eq!(before.misses, after.misses);
        assert_eq!(before.bytes_read, after.bytes_read);
        assert_eq!(before.resident_expert_bytes, after.resident_expert_bytes);
        drop(mapped);
        std::fs::remove_file(path).unwrap();
    }
}
