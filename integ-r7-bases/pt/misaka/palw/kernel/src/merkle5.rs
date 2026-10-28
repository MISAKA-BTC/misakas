//! Flat-v5 INTERNAL trace material, including I64/I128 non-commit intermediates.
//! This is a new hash namespace; v4 continues to refuse wide dtypes. Program
//! validation still prohibits wide commit points. No node descriptor selects it.
use crate::hash::Digest;
pub use crate::merkle4::{FlatDescentV4 as FlatDescentV5, FlatLayoutV4 as FlatLayoutV5, FlatMaterialBoundsV4 as FlatMaterialBoundsV5};
use borsh::BorshDeserialize;
use misaka_palw_tir::{DType, Tensor};
use std::io::{self, Cursor};
pub type FlatTreeV5 = crate::merkle4::FlatTree<5>;
pub type FlatLeafHashV5 = crate::merkle4::FlatLeafHash<5>;
pub type FlatLeafValueV5 = crate::merkle4::FlatLeafValue<5>;
pub const FLAT_LEAF_ELEMENTS_V5: u64 = 64;
pub const FLAT_LEAF_MAX_ELEMENTS_V5: usize = 127;
pub const FLAT_HASH_MAX_BYTES_V5: usize = crate::merkle4::FLAT_HASH_MAX_BYTES_V4;
pub const FLAT_VALUE_MAX_BYTES_V5: usize = FLAT_HASH_MAX_BYTES_V5 + 4 + 127 * 16;
pub const FLAT_COMMITMENT_DOMAIN_V5: &[u8] = b"misaka-palw/kernel/tensor/flat/v5";
pub const FLAT_LEAF_DOMAIN_V5: &[u8] = b"misaka-palw/kernel/tensor-leaf/flat/v5";
pub const FLAT_NODE_DOMAIN_V5: &[u8] = b"misaka-palw/kernel/tensor-node/flat/v5";
pub fn flat_material_bounds_v5(dtype: DType, shape: &[u64]) -> Option<FlatMaterialBoundsV5> {
    FlatLayoutV5::of(shape)?.bounds_for::<5>(dtype)
}
pub fn flat_commitment_from_bytes_v5(dtype: DType, shape: &[u64], bytes: &[u8]) -> io::Result<Digest> {
    crate::merkle4::flat_commitment_from_bytes::<5>(dtype, shape, bytes)
}
pub fn flat_commitment_from_tensor_v5(t: &Tensor) -> io::Result<Digest> {
    crate::merkle4::flat_commitment_from_tensor::<5>(t)
}
fn decode<T: BorshDeserialize>(bytes: &[u8], limit: usize) -> io::Result<T> {
    let bad = || io::Error::new(io::ErrorKind::InvalidData, "invalid bounded flat-v5 opening");
    if bytes.len() > limit {
        return Err(bad());
    }
    let mut r = Cursor::new(bytes);
    let out = T::deserialize_reader(&mut r)?;
    if r.position() != bytes.len() as u64 {
        return Err(bad());
    }
    Ok(out)
}
pub fn decode_flat_hash_v5(bytes: &[u8]) -> io::Result<FlatLeafHashV5> {
    decode(bytes, FLAT_HASH_MAX_BYTES_V5)
}
pub fn decode_flat_value_v5(bytes: &[u8]) -> io::Result<FlatLeafValueV5> {
    decode(bytes, FLAT_VALUE_MAX_BYTES_V5)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::{finish, keyed};
    use crate::merkle4::{FlatTreeV4, decode_flat_value_v4};
    // Independent preimages and recursive pair tree, not the generic stack/builder.
    fn oracle(t: &Tensor) -> Digest {
        fn tree(xs: &[Digest]) -> Digest {
            if xs.len() == 1 {
                return xs[0];
            }
            let next: Vec<_> = xs
                .chunks(2)
                .map(|p| {
                    if p.len() == 1 {
                        p[0]
                    } else {
                        let mut h = keyed(FLAT_NODE_DOMAIN_V5);
                        h.update(&p[0]).update(&p[1]);
                        finish(h)
                    }
                })
                .collect();
            tree(&next)
        }
        let leaves: Vec<_> = (0..t.data.len() / 64)
            .map(|i| {
                let count = if i + 1 == t.data.len() / 64 { t.data.len() - i * 64 } else { 64 };
                let mut bytes = vec![t.dtype.tag()];
                bytes.extend_from_slice(&(i as u64).to_le_bytes());
                bytes.extend_from_slice(&(count as u64).to_le_bytes());
                for v in &t.data[i * 64..i * 64 + count] {
                    t.dtype.encode_le(*v, &mut bytes);
                }
                let mut h = keyed(FLAT_LEAF_DOMAIN_V5);
                h.update(&bytes);
                finish(h)
            })
            .collect();
        let mut h = keyed(FLAT_COMMITMENT_DOMAIN_V5);
        h.update(&[t.dtype.tag(), t.shape.len() as u8]);
        for d in &t.shape {
            h.update(&(*d as u64).to_le_bytes());
        }
        h.update(&tree(&leaves));
        finish(h)
    }
    #[test]
    fn flat_v5_internal_all_dtypes_and_extreme_wide_values_match_independent_oracle() {
        for d in DType::ALL {
            for n in [64, 65, 127, 128, 129, 257, 1025, 4097] {
                let mut vs: Vec<_> = (0..n).map(|i| if d == DType::Idx { i as i128 } else { (i % 127) as i128 - 63 }).collect();
                vs[0] = d.min_value();
                vs[n - 1] = d.max_value();
                let t = Tensor::new(d, vec![n], vs).unwrap();
                let root = oracle(&t);
                let tree = FlatTreeV5::of(&t).unwrap();
                assert_eq!(tree.commitment(), root);
                assert_eq!(flat_commitment_from_bytes_v5(d, &[n as u64], &t.to_le_bytes()).unwrap(), root);
                assert_eq!(flat_commitment_from_tensor_v5(&t).unwrap(), root);
                for i in 0..tree.layout().leaves() {
                    let value = FlatLeafValueV5::of(&t, &tree, i).unwrap();
                    assert!(value.authenticates(&root));
                    let bytes = borsh::to_vec(&value).unwrap();
                    assert_eq!(decode_flat_value_v5(&bytes).unwrap(), value);
                    assert!(bytes.len() as u64 <= flat_material_bounds_v5(d, &[n as u64]).unwrap().leaf_value_bytes);
                    let mut bad = value.clone();
                    bad.bytes[0] ^= 1;
                    assert!(!bad.authenticates(&root));
                    assert!(decode_flat_value_v5(&borsh::to_vec(&bad).unwrap()).is_err());
                    if !d.committable() {
                        assert!(decode_flat_value_v4(&bytes).is_err());
                    }
                }
                if d.committable() {
                    assert_ne!(FlatTreeV4::of(&t).unwrap().commitment(), root);
                } else {
                    assert!(FlatTreeV4::of(&t).is_err());
                }
            }
        }
    }
    #[test]
    fn flat_v5_wide_descent_and_strict_carriers_preserve_tail_and_unknown_version_refusals() {
        let t = Tensor::new(DType::I128, vec![5, 13], vec![i128::MAX; 65]).unwrap();
        let tree = FlatTreeV5::of(&t).unwrap();
        let value = FlatLeafValueV5::of(&t, &tree, 0).unwrap();
        let bytes = borsh::to_vec(&value).unwrap();
        for cut in 0..bytes.len() {
            assert!(decode_flat_value_v5(&bytes[..cut]).is_err());
        }
        let mut bad = bytes.clone();
        bad.push(0);
        assert!(decode_flat_value_v5(&bad).is_err());
        let b = flat_material_bounds_v5(DType::I128, &[65]).unwrap();
        assert_eq!(b.leaf_elements, 65);
        assert_eq!(b.source_bytes, 65 * 16);
        assert!(FlatTreeV5::of_bounded(&t, b.retained_tree_bytes - 1).is_err());
        assert!(FlatTreeV5::of_bounded(&t, b.retained_tree_bytes).is_ok());
        assert!(crate::merkle4::FlatTree::<6>::of(&t).is_err());
        let t = Tensor::new(DType::I64, vec![513], vec![7; 513]).unwrap();
        let expected = FlatTreeV5::of(&t).unwrap();
        for i in 0..expected.layout().leaves() {
            let mut bad = t.clone();
            let (first, count) = expected.layout().span(i).unwrap();
            bad.data[first as usize + count - 1] += 1;
            let actual = FlatTreeV5::of(&bad).unwrap();
            let mut at = 0;
            let mut found = false;
            for _ in 0..flat_material_bounds_v5(DType::I64, &[513]).unwrap().localization_replies {
                let reply = actual.opening(at).unwrap();
                assert_eq!(decode_flat_hash_v5(&borsh::to_vec(&reply).unwrap()).unwrap(), reply);
                match reply.descend(&actual.commitment(), &expected).unwrap() {
                    FlatDescentV5::Probe(next) => at = next,
                    FlatDescentV5::DivergentLeaf(index) => {
                        assert_eq!(index, i);
                        found = true;
                        break;
                    }
                    FlatDescentV5::Matched => panic!("wrong wide tree matched"),
                }
            }
            assert!(found);
        }
    }
}
