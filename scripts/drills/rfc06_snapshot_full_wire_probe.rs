//! Full state + authentic declaration codec measurement for the native fixture.
//! Independent tool, outside final-test-v2-build-inputs.json. Use libraries from
//! the same native producer binary. No state is installed and no P2P is exercised.
//! Usage: probe native-carriage.borsh authentic-declarations.borsh
#![allow(dead_code)]
mod palw_gossip {
    pub const PALW_PRUNING_STATE_MAX_BYTES: usize = 384 << 20;
}
mod flow_context {
    pub const PROTOCOL_VERSION_PALW_SNAPSHOT_COMPRESSED: u32 = 107;
}
#[path = "rfc06_snapshot_declarations_dump.rs"]
mod authentic_declarations;
#[path = "../../protocol/flows/src/palw_snapshot_chunks.rs"]
mod palw_snapshot_chunks;

use borsh::BorshDeserialize;
use kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2;
use palw_snapshot_chunks::{DECODED_SNAPSHOT_MAX_BYTES, SnapshotReceiver, SnapshotSender, SnapshotSpill, chunk_hash_profile};
use std::{
    fs::File,
    io::{BufReader, Read},
    time::Instant,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let source = args.next().ok_or("native canonical carriage path required")?;
    let declaration_source = args.next().ok_or("authentic declaration file required; state-only mode is not offered")?;
    if args.next().is_some() {
        return Err("unexpected arguments".into());
    }
    let source = File::open(source)?;
    let source_bytes = source.metadata()?.len();
    if source_bytes > DECODED_SNAPSHOT_MAX_BYTES {
        return Err("canonical native source exceeds experimental cap".into());
    }
    let carriage = PalwStateCarriageV2::deserialize_reader(&mut BufReader::new(source))?;
    let point = carriage.last_point.as_ref().ok_or("native source lacks an actual block context")?.block;
    let (base, classes) = authentic_declarations::fixture_classes()?;
    let authentic = authentic_declarations::complete_declarations(&carriage, base, &classes)?;
    let declaration_vec_bytes = borsh::object_length(&authentic)? as u64;
    let file = File::open(declaration_source)?;
    // Check the known, fully derived count BEFORE reading an arbitrary file.
    if file.metadata()?.len() != declaration_vec_bytes {
        return Err("declaration file length differs from the complete authentic fixture vector".into());
    }
    let mut reader = BufReader::new(file);
    let declarations = Vec::<(kaspa_hashes::Hash64, Vec<u8>)>::deserialize_reader(&mut reader)?;
    let mut extra = [0u8; 1];
    if reader.read(&mut extra)? != 0 || declarations != authentic {
        return Err("declaration bytes differ from the complete authentic genesis admissions".into());
    }
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
    let (decoded, received_declarations) = receiver.decode_snapshot()?;
    if decoded != carriage {
        return Err("canonical state changed in transport".into());
    }
    if received_declarations != declarations {
        return Err("authentic declaration bytes changed or were filtered in transport".into());
    }
    println!(
        "{{\"schemaVersion\":1,\"kind\":\"rfc06-native-full-snapshot-wire-probe\",\"sourceBytes\":{source_bytes},\"canonicalEnvelopeBytes\":{},\"compressedWireBytes\":{wire},\"wireCapBytes\":{},\"canonicalCapBytes\":{DECODED_SNAPSHOT_MAX_BYTES},\"chunks\":{count},\"declarationVecBytes\":{declaration_vec_bytes},\"nativeClassesMatched\":3,\"declarationsOffered\":{},\"declarationsRetained\":{},\"builtinFloorAdmissionNone\":1,\"builtinFloorClassification\":\"binary-derived-profile-and-pinned-root\",\"builtinFloorWeightsRecomputed\":false,\"encodeMs\":{encode_ms},\"receiveDecodeMs\":{},\"stateEquality\":true,\"declarationBytesEquality\":true,\"fullFixtureSnapshotMeasured\":true,\"actualServerIndexChecked\":false,\"realP2pAccepted\":false}}",
        profile.decoded,
        palw_gossip::PALW_PRUNING_STATE_MAX_BYTES,
        declarations.len(),
        received_declarations.len(),
        begun.elapsed().as_millis(),
    );
    Ok(())
}
