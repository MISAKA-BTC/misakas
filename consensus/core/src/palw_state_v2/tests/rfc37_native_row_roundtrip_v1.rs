//! Transport bridge for the EXISTING actual ingress/carrier lifecycle fixture.
//! This adds no genesis, source approval, or node/header authority. Full ordinary
//! peer import remains a separate node fixture; no class declaration is offered
//! by this retained-native-carriage codec check.
use super::*;
use borsh::BorshDeserialize;
use std::io::{Read, Seek, SeekFrom, Write};
use std::sync::mpsc::{SyncSender, sync_channel};

const PIECE: usize = 64 << 10;
enum Frame {
    Total(u64),
    Bytes(Vec<u8>),
    Done(u64),
}
struct Writer {
    tx: SyncSender<Frame>,
    pending: Vec<u8>,
}
impl Writer {
    fn send(&mut self) -> std::io::Result<()> {
        if !self.pending.is_empty() {
            let bytes = std::mem::replace(&mut self.pending, Vec::with_capacity(PIECE));
            self.tx
                .send(Frame::Bytes(bytes))
                .map_err(|_| std::io::Error::new(std::io::ErrorKind::BrokenPipe, "row consumer stopped"))?;
        }
        Ok(())
    }
}
impl Write for Writer {
    fn write(&mut self, mut bytes: &[u8]) -> std::io::Result<usize> {
        let total = bytes.len();
        while !bytes.is_empty() {
            let n = bytes.len().min(PIECE - self.pending.len());
            self.pending.extend_from_slice(&bytes[..n]);
            bytes = &bytes[n..];
            if self.pending.len() == PIECE {
                self.send()?;
            }
        }
        Ok(total)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.send()
    }
}

pub(super) fn roundtrip(state: &PalwChainStateV2, params: &PalwStateParamsV2) -> crate::palw_native_resource_v1::NativeChainStateV1 {
    use crate::palw_native_resource_v1::*;
    use crate::palw_row_stream_v1::PalwRowGenerationProgressV1;
    let route = state.kernel_route().expect("actual lifecycle has native scoped state");
    let table = crate::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1;
    let mut descriptor = [0u8; 65];
    descriptor[0] = b'D';
    assert_eq!(&route.aux.get(&(table, descriptor.to_vec())).expect("actual D3 descriptor")[..2], &[3, 0]);
    assert!(
        route.aux.iter().any(|((t, key), raw)| *t == table && key.first() == Some(&b'I') && {
            let root: crate::palw_claim_local_context_nodes_v2::ComponentRootV2 = borsh::from_slice(raw).unwrap();
            root.spec.owner == 9 && root.rows > 0
        }),
        "actual nonempty carrier9 component is required"
    );
    let mut events = 0;
    for ((t, key), _) in &route.aux {
        if *t == table && key.len() == 65 && key[0] == b'F' {
            let block = Hash64::from_bytes(key[1..].try_into().unwrap());
            assert!(
                crate::palw_state_v2::palw_claim_local_native_frame_events_v10::visit_rooted_native_events_v10(
                    state,
                    block,
                    |_, _, _| {
                        events += 1;
                        Ok(())
                    }
                )
                .unwrap()
            );
        }
    }
    assert!(events > 0, "actual V10 native events are required");
    let _projection = reserve_native_requests_v1(native_value_requests_v1(state).unwrap()).unwrap();
    let carriage = PalwStateCarriageV2::from_state(state);
    let n = borsh::object_length(&carriage).unwrap();
    let native_bytes = u64::try_from(n).unwrap();
    // Same concrete root workspace as the production pinned snapshot exporter,
    // admitted before state_root/preimage can allocate their serialization scratch.
    let _root_workspace = reserve_native_root_workspace_v1(native_bytes).unwrap();
    let suffix_bytes = native_bytes.checked_add(14).expect("native envelope extent");
    let pieces = (PIECE as u64).checked_mul(4).expect("bounded channel buffers");
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<u8>(native_bytes).unwrap();
    q.exact_v1::<u8>(suffix_bytes).unwrap();
    q.exact_v1::<u8>(pieces).unwrap();
    let _scratch = reserve_native_requests_v1(q.bytes_v1()).unwrap();
    let mut canonical = Vec::with_capacity(n);
    borsh::BorshSerialize::serialize(&carriage, &mut canonical).unwrap();
    assert_eq!(canonical.len(), n);
    let mut suffix = Vec::with_capacity(usize::try_from(suffix_bytes).unwrap());
    suffix.extend_from_slice(&1u16.to_le_bytes());
    suffix.extend_from_slice(&(n as u64).to_le_bytes());
    suffix.extend_from_slice(&canonical);
    suffix.extend_from_slice(&0u32.to_le_bytes());
    let point = state.last_point().unwrap().block;
    let root = state.state_root();
    let mut spill = tempfile::tempfile().unwrap();
    let verified = std::thread::scope(|scope| {
        let (tx, rx) = sync_channel::<Frame>(1);
        let suffix_ref = &suffix;
        let worker = scope.spawn(move || -> std::io::Result<u64> {
            let total = crate::palw_direct_row_export_v1::write_row_generation_stream_v1(
                state,
                &mut std::io::Cursor::new(suffix_ref),
                suffix_ref.len() as u64,
                &mut std::io::sink(),
            )?;
            tx.send(Frame::Total(total)).map_err(|_| std::io::Error::other("row consumer stopped"))?;
            let mut writer = Writer { tx, pending: Vec::with_capacity(PIECE) };
            let written = crate::palw_direct_row_export_v1::write_row_generation_stream_v1(
                state,
                &mut std::io::Cursor::new(suffix_ref),
                suffix_ref.len() as u64,
                &mut writer,
            )?;
            writer.flush()?;
            writer.tx.send(Frame::Done(written)).map_err(|_| std::io::Error::other("row consumer stopped"))?;
            Ok(written)
        });
        let received = (|| -> Result<_, String> {
            let total = match rx.recv_timeout(std::time::Duration::from_secs(30)).map_err(|e| e.to_string())? {
                Frame::Total(n) => n,
                _ => return Err("generation header order".into()),
            };
            let mut progress = PalwRowGenerationProgressV1::new(root, total).map_err(|e| e.to_string())?;
            let mut available = 0;
            loop {
                match rx.recv_timeout(std::time::Duration::from_secs(30)).map_err(|e| e.to_string())? {
                    Frame::Bytes(bytes) => {
                        if bytes.is_empty() || bytes.len() > PIECE {
                            return Err("bounded channel piece".into());
                        }
                        spill.seek(SeekFrom::End(0)).map_err(|e| e.to_string())?;
                        spill.write_all(&bytes).map_err(|e| e.to_string())?;
                        available += bytes.len() as u64;
                        progress.advance(&mut spill, available).map_err(|e| e.to_string())?;
                    }
                    Frame::Done(n) => {
                        if n != total || available != total {
                            return Err("generation extent".into());
                        }
                        break;
                    }
                    _ => return Err("generation metadata repeated".into()),
                }
            }
            progress.suffix().ok_or_else(|| "incremental collection verification incomplete".into())
        })();
        // A consumer refusal closes the only channel before joining its worker.
        drop(rx);
        let written = worker.join().expect("canonical writer stopped");
        match (received, written) {
            (Err(why), _) => panic!("incremental native row receive refused: {why}"),
            (Ok(_), Err(why)) => panic!("canonical native row export refused: {why}"),
            (Ok(verified), Ok(_)) => verified,
        }
    });
    spill.seek(SeekFrom::Start(verified.offset)).unwrap();
    let shape = preflight_native_snapshot_v1((&mut spill).take(verified.length), verified.length).unwrap();
    let _received_owner = reserve_native_wire_v1(point, &shape).unwrap();
    spill.seek(SeekFrom::Start(verified.offset)).unwrap();
    assert_eq!(u16::deserialize_reader(&mut spill).unwrap(), 1);
    let state_bytes = u64::deserialize_reader(&mut spill).unwrap();
    let mut frame = (&mut spill).take(state_bytes);
    let decoded = PalwStateCarriageV2::deserialize_reader(&mut frame).unwrap();
    assert_eq!(frame.limit(), 0);
    drop(frame);
    assert_eq!(u32::deserialize_reader(&mut spill).unwrap(), 0);
    assert_eq!(spill.stream_position().unwrap(), verified.offset + verified.length);
    let restored = restore_native_carriage_v1(point, decoded, params, Some(root), false, None).unwrap();
    assert!(verified.inventory.matches_state(&restored).unwrap(), "every D3/carrier/native collection retained");
    assert_eq!(restored.state_root(), root);
    // A fully valid native suffix cannot compensate for an omitted D3/carrier
    // collection stream, even when its flat preimage and whole state are valid.
    let preimage = state.state_root_preimage();
    let stream_bytes = u64::try_from(preimage.len()).unwrap().checked_add(9).expect("empty inventory framing");
    let omitted_bytes = 22u64
        .checked_add(stream_bytes)
        .and_then(|n| n.checked_add(8))
        .and_then(|n| n.checked_add(suffix_bytes))
        .expect("omitted inventory generation extent");
    let mut q = TirNativeRequestsV1::new();
    q.vec_v1::<u8>(stream_bytes).unwrap(); // encoder's growing collection-frame Vec
    q.exact_v1::<u8>(omitted_bytes).unwrap(); // encoder's exact output Vec
    let _omission = reserve_native_requests_v1(q.bytes_v1()).unwrap();
    let omitted = crate::palw_row_stream_v1::encode_row_stream_generation_v1(&preimage, &[], &suffix).unwrap();
    let token = crate::palw_row_stream_v1::palw_verify_row_generation_reader_v1(
        root,
        &mut std::io::Cursor::new(&omitted),
        omitted.len() as u64,
    )
    .unwrap();
    assert!(!token.inventory.matches_state(&restored).unwrap(), "D3 native suffix is not collection-inventory authority");
    restored
}
