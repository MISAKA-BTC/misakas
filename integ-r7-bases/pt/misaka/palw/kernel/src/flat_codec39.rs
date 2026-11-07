//! Small shared allocation-before-decode grammar for original39 action bodies.
use borsh::{BorshDeserialize, BorshSerialize};
use std::io::{Cursor, Read};
pub(crate) struct Reader39<'a> {
    pub cursor: Cursor<&'a [u8]>,
}
impl<'a> Reader39<'a> {
    pub fn new(raw: &'a [u8], cap: usize) -> Result<Self, String> {
        if raw.len() > cap {
            return Err("owning39 body exceeds original typed cap".into());
        }
        Ok(Self { cursor: Cursor::new(raw) })
    }
    pub fn get<T: BorshDeserialize>(&mut self) -> Result<T, String> {
        T::deserialize_reader(&mut self.cursor).map_err(|_| "owning39 truncated/unknown canonical field".into())
    }
    pub fn count(&mut self, cap: usize, width: usize) -> Result<usize, String> {
        let n = self.get::<u32>()? as usize;
        if n > cap || n > self.cursor.get_ref().len().saturating_sub(self.cursor.position() as usize) / width {
            return Err("owning39 count exceeds original preallocation extent".into());
        }
        Ok(n)
    }
    pub fn words(&mut self, cap: usize) -> Result<Vec<u32>, String> {
        let n = self.count(cap, 4)?;
        let mut v = Vec::with_capacity(n);
        for _ in 0..n {
            v.push(self.get()?);
        }
        Ok(v)
    }
    pub fn blob(&mut self, cap: usize) -> Result<Vec<u8>, String> {
        let n = self.count(cap, 1)?;
        let mut b = vec![0; n];
        self.cursor.read_exact(&mut b).map_err(|_| "owning39 blob extent")?;
        Ok(b)
    }
    pub fn finish<T: BorshSerialize>(&self, value: &T) -> Result<(), String> {
        if self.cursor.position() != self.cursor.get_ref().len() as u64
            || borsh::to_vec(value).map_err(|e| e.to_string())? != *self.cursor.get_ref()
        {
            return Err("owning39 surplus or noncanonical body".into());
        }
        Ok(())
    }
}
