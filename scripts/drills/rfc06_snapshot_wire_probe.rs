//! Codec measurement of an actual native carriage saved by the four-population test.
//! Build directly against the matching node-target dependencies; no new Cargo graph.
//! Usage: probe native-carriage.borsh [class-declarations.borsh]
//! No native state is installed and this is not a P2P/pruning acceptance test.
#![allow(dead_code)]
mod palw_gossip {
    pub const PALW_PRUNING_STATE_MAX_BYTES: usize = 384 << 20;
}
mod flow_context {
    pub const PROTOCOL_VERSION_PALW_SNAPSHOT_COMPRESSED: u32 = 107;
}
#[path = "../../protocol/flows/src/palw_snapshot_chunks.rs"]
mod palw_snapshot_chunks;

use borsh::BorshDeserialize;
use kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2;
use palw_snapshot_chunks::{DECODED_SNAPSHOT_MAX_BYTES, SnapshotReceiver, SnapshotSender, SnapshotSpill, chunk_hash_profile};
use std::{
    fs::File,
    io::{self, BufReader},
    time::Instant,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = args.next().ok_or("native canonical carriage path required")?;
    let declarations_path = args.next();
    if args.next().is_some() {
        return Err("unexpected arguments".into());
    }
    let file = File::open(&path)?;
    let source_bytes = file.metadata()?.len();
    if source_bytes > DECODED_SNAPSHOT_MAX_BYTES {
        return Err("canonical source exceeds experimental cap".into());
    }
    let carriage = PalwStateCarriageV2::deserialize_reader(&mut BufReader::new(file))?;
    let declarations: Vec<(kaspa_hashes::Hash64, Vec<u8>)> = match declarations_path {
        Some(path) => borsh::from_slice(&std::fs::read(path)?)?,
        None => Vec::new(),
    };
    let point = carriage.last_point.as_ref().ok_or("native carriage lacks actual block context")?.block;
    let begun = Instant::now();
    let mut sender = SnapshotSender::encode_snapshot_compressed(SnapshotSpill::new()?, point, &carriage, &declarations, [0x73; 32])?;
    let profile = sender.profile.ok_or("compressed sender missing profile")?;
    let wire = sender.total;
    let count = sender.count;
    let root = sender.root;
    let encode_ms = begun.elapsed().as_millis();
    let begun = Instant::now();
    let mut receiver = SnapshotReceiver::new_compressed(SnapshotSpill::new()?, point, root, wire, count, profile)?;
    for index in 0..count {
        let bytes = sender.chunk(point, (index > 0).then_some(root), index)?;
        receiver.accept(
            point,
            root,
            wire,
            count,
            index,
            chunk_hash_profile(point, root, wire, count, index, &bytes, Some(profile)),
            &bytes,
        )?;
    }
    let (decoded, filtered) = receiver.decode_snapshot()?;
    if decoded != carriage {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "canonical state changed in transport").into());
    }
    println!(
        "{{\"schemaVersion\":1,\"kind\":\"rfc06-native-snapshot-wire-probe\",\"sourceBytes\":{source_bytes},\"canonicalEnvelopeBytes\":{},\"compressedWireBytes\":{wire},\"wireCapBytes\":{},\"canonicalCapBytes\":{DECODED_SNAPSHOT_MAX_BYTES},\"chunks\":{count},\"declarationsOffered\":{},\"declarationsRetained\":{},\"encodeMs\":{encode_ms},\"receiveDecodeMs\":{},\"canonicalEquality\":true,\"realP2pAccepted\":false}}",
        profile.decoded,
        palw_gossip::PALW_PRUNING_STATE_MAX_BYTES,
        declarations.len(),
        filtered.len(),
        begun.elapsed().as_millis()
    );
    Ok(())
}
