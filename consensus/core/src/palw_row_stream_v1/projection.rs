//! Wire projections for the two class collections whose full records are not their rooted bytes.
//! Scan the carried program/class without reconstructing it. Full semantic and hash validation
//! still belongs to the whole-state import gate.
use super::PalwRowProjectionV1;
use crate::{
    Hash64,
    palw_native_resource_v1::{NativeWireReaderV1, NativeWireScanV1},
};
use std::io;

fn project(full: &[u8], tir: bool) -> io::Result<Vec<u8>> {
    let mut reader = NativeWireReaderV1::new(full, full.len() as u64);
    macro_rules! scan {
        ($($ty:ty),+ $(,)?) => { $(<$ty>::scan_native_wire_v1(&mut reader)?;)+ };
    }
    if tir {
        scan!(u16, crate::palw_tir_attempt_v1::PalwTirJobFactsV1, Hash64, Hash64, Hash64, Hash64, u32, u32, Vec<(u8, u16)>);
    } else {
        scan!(
            u16,
            Hash64,
            Hash64,
            u8,
            Hash64,
            Hash64,
            Hash64,
            Vec<crate::palw_gen_class_v1::PalwGenImageOfferV1>,
            Option<u32>,
            u64,
            Vec<(u8, u8, u16)>
        );
    }
    let prefix = full.len() - reader.remaining() as usize;
    if tir {
        scan!(Vec<u8>);
    } else {
        scan!(crate::palw_gen_class_v1::PalwGenClassV1);
    }
    if reader.remaining() != 0 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "class row has trailing bytes"));
    }
    let mut rooted = full[..prefix].to_vec();
    if tir {
        // TIR roots the record with an empty program; GEN omits the class field entirely.
        rooted.extend_from_slice(&0u32.to_le_bytes());
    }
    Ok(rooted)
}

pub(super) fn for_label(label: &[u8]) -> Option<PalwRowProjectionV1> {
    let tir = match label {
        b"tir_classes" => true,
        b"gen_classes" => false,
        _ => return None,
    };
    Some(Box::new(move |full| project(full, tir).map_err(|err| err.to_string())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_row_stream_v1::*;
    use crate::palw_state_v2::{palw_collection_root_of_entries_v1, palw_state_root_of_preimage_v1};

    fn roundtrip(label: &[u8], full: Vec<u8>, rooted: Vec<u8>) {
        assert_eq!(for_label(label).unwrap()(&full).unwrap(), rooted);
        let key = borsh::to_vec(&Hash64::from_bytes([3; 64])).unwrap();
        let preimage = palw_collection_root_of_entries_v1(label, 1, std::iter::once((key.clone(), rooted))).as_byte_slice().to_vec();
        let root = palw_state_root_of_preimage_v1(&preimage);
        let collections = [PalwRowStreamCollectionV1 { label: label.to_vec(), offset: 0, rows: vec![(key, full.clone())] }];
        let frame = encode_row_stream_generation_v1(&preimage, &collections, b"staging").unwrap();
        let accepted = palw_accept_row_stream_generation_v1(root, &frame, Vec::new(), frame.len() as u64, None).unwrap();
        let spill = accepted.verified.into_sink();
        assert_eq!(palw_read_row_spill_v1(spill.as_slice()).unwrap()[0].1[0].1, full);
        let mut trailing = full.clone();
        trailing.push(0);
        assert!(for_label(label).unwrap()(&trailing).is_err());
        assert!(for_label(label).unwrap()(&full[..full.len() - 1]).is_err());
    }

    #[test]
    fn generation_projects_full_tir_record_without_allocating_its_program() {
        let mut row = crate::palw_tir_admission_v1::PalwTirClassRecordV1::test_row_v1(Hash64::from_bytes([4; 64]));
        row.dissected = vec![(1, 2), (3, 4)];
        row.program = std::sync::Arc::new(vec![0x73; 65_536]);
        row.program_bytes = row.program.len() as u32;
        roundtrip(b"tir_classes", borsh::to_vec(&row).unwrap(), row.rooted_bytes_v1());
    }

    #[test]
    fn generation_projects_full_gen_record_without_reconstructing_its_class() {
        let mut row = crate::palw_gen_class_v1::PalwGenClassRecordV1::test_row_v1(5);
        row.images = vec![crate::palw_gen_class_v1::PalwGenImageOfferV1 { h: 3, w: 7, tile_len: 4, token_equivalents: 1 }];
        row.dissected = vec![(1, 2, 3)];
        roundtrip(b"gen_classes", borsh::to_vec(&row).unwrap(), row.rooted_bytes_v1());
    }
}
