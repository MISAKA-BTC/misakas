//! Original producer response pages. A reply unit owns at most two COMPLETE
//! answer attempts. First part consumes the full prepaid Source16 answer price,
//! including all4 fragments and assembly, before allocation. A bad part burns
//! that original attempt; no retry manufactures new fragment/assembly credit.
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::hash::{Digest, object_id};
pub const REPLY_PART_PREFIX_V29: usize = 143;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BorrowedNativeReplyPartV29<'a> {
    pub pending: Digest,
    pub group: Digest,
    pub index: u16,
    pub count: u16,
    pub total: u32,
    pub bytes: &'a [u8],
}
pub fn borrowed_native_reply_metadata_v29(raw: &[u8]) -> Option<BorrowedNativeReplyPartV29<'_>> {
    if raw.len() < 143 || raw.len() > 143 + (32 << 10) || raw[..2] != 29u16.to_le_bytes() || raw[2] != 0 {
        return None;
    }
    let index = u16::from_le_bytes(raw[131..133].try_into().ok()?);
    let count = u16::from_le_bytes(raw[133..135].try_into().ok()?);
    let total = u32::from_le_bytes(raw[135..139].try_into().ok()?);
    if count == 0 || count > 4 || index >= count || total == 0 || total > 128 << 10 || total.div_ceil(32 << 10) != count as u32 {
        return None;
    }
    Some(BorrowedNativeReplyPartV29 {
        pending: raw[3..67].try_into().ok()?,
        group: raw[67..131].try_into().ok()?,
        index,
        count,
        total,
        bytes: &raw[143..],
    })
}
pub fn borrowed_native_reply_part_v29(raw: &[u8]) -> Option<BorrowedNativeReplyPartV29<'_>> {
    let p = borrowed_native_reply_metadata_v29(raw)?;
    let n = u32::from_le_bytes(raw[139..143].try_into().ok()?) as usize;
    let expected = if p.index + 1 == p.count { p.total as usize - p.index as usize * (32 << 10) } else { 32 << 10 };
    (n == expected && p.bytes.len() == n).then_some(p)
}
pub fn native_reply_group_v29(pending: Digest, bytes: &[u8]) -> Digest {
    object_id(b"misaka-palw/native/original-producer-reply-pages/v29", &(pending, bytes))
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeReplyProgressV29 {
    pub version: u16,
    pub pending: Digest,
    pub producer: Digest,
    pub original_claim: Digest,
    pub original_source: Digest,
    pub original_lease: Digest,
    pub attempt: u8,
    pub group: Digest,
    pub count: u16,
    pub total: u32,
    pub accepted: u8,
    pub burned: bool,
    pub completed: bool,
}
impl NativeReplyProgressV29 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/original-response-attempt/v29", self)
    }
    pub fn check(&self) -> Result<(), String> {
        if self.version != 29
            || self.attempt == 0
            || self.attempt > 2
            || self.count == 0
            || self.count > 4
            || self.total == 0
            || self.total > 128 << 10
            || self.total.div_ceil(32 << 10) != self.count as u32
            || self.accepted >> self.count != 0
            || [self.pending, self.producer, self.original_claim, self.original_source, self.original_lease, self.group]
                .contains(&[0; 64])
            || self.completed && self.accepted != ((1u8 << self.count) - 1)
            || self.burned && self.completed
        {
            return Err("reply29 original unit/attempt/framing/replay conservation differs".into());
        }
        Ok(())
    }
}
