//! RFC-0037 §2.1/§5.1 coverage: a stream that matched every collection it NAMED may still have left one out. The
//! receiver cannot list the slots from the preimage alone (fence-dependent layout), but a caller that holds a codec
//! table can name the exact `(label, preimage offset)` set it requires. A missing, extra, or mislocated collection is
//! then refused here as a typed peer fault, before the whole-state gate spends memory on an incomplete spill.
//! This does not replace the whole-state gate; it is not evidence of validity.
use super::{PalwRowStreamErrorV1, PalwRowStreamVerifiedV1, PalwRowStreamVerifierV1};
use std::collections::BTreeMap;
use std::io::Write;

impl<W: Write> PalwRowStreamVerifierV1<W> {
    /// [`Self::finish`], but the closed collections must be exactly `required` (`label`, preimage offset).
    pub fn finish_covering(self, required: &[(&[u8], usize)]) -> Result<PalwRowStreamVerifiedV1<W>, PalwRowStreamErrorV1> {
        if self.poisoned {
            return Err(PalwRowStreamErrorV1::Poisoned);
        }
        if self.open.is_some() {
            return Err(PalwRowStreamErrorV1::CollectionAlreadyOpen);
        }
        let mut want = BTreeMap::new();
        for (label, offset) in required {
            if want.insert(label.to_vec(), *offset).is_some() {
                return Err(PalwRowStreamErrorV1::RequiredLayoutDuplicate(label.to_vec()));
            }
        }
        // `done` and `used` are pushed together, one per closed collection, in stream order.
        for ((label, _), got) in self.done.iter().zip(self.used.iter()) {
            match want.remove(label) {
                None => return Err(PalwRowStreamErrorV1::UnexpectedCollection(label.clone())),
                Some(expected) if expected != *got => {
                    return Err(PalwRowStreamErrorV1::CollectionOffsetDiffers { label: label.clone(), expected, got: *got });
                }
                Some(_) => {}
            }
        }
        if let Some((label, _)) = want.into_iter().next() {
            return Err(PalwRowStreamErrorV1::MissingCollection(label));
        }
        self.finish()
    }
}
