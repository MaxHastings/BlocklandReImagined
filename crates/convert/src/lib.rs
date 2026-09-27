//! Offline-only legacy readers. Never execute source scripts.
pub mod archive;
pub mod avatar;
pub mod bls;
pub mod brick;
pub mod catalog;
pub mod collision;
pub mod effect_bindings;
pub mod effect_script;
pub mod effects;
pub mod environment;
pub mod events;
pub mod interior;
pub mod lighting;
pub mod mission;
pub mod shape;
pub mod terrain;
pub mod water;

use anyhow::{Result, bail, ensure};

#[derive(Clone)]
pub struct Reader<'a> {
    data: &'a [u8],
    offset: usize,
}
impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }
    pub fn bytes(&mut self, count: usize) -> Result<&'a [u8]> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or_else(|| anyhow::anyhow!("Length overflow"))?;
        if end > self.data.len() {
            bail!(
                "Truncated input at byte {}: need {count}, remaining {}",
                self.offset,
                self.data.len() - self.offset
            );
        }
        let data = &self.data[self.offset..end];
        self.offset = end;
        Ok(data)
    }
    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.bytes(1)?[0])
    }
    pub fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.bytes(2)?.try_into()?))
    }
    pub fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.bytes(4)?.try_into()?))
    }
    pub fn i32(&mut self) -> Result<i32> {
        Ok(self.u32()? as i32)
    }
    pub fn f32(&mut self) -> Result<f32> {
        let v = f32::from_bits(self.u32()?);
        ensure!(v.is_finite(), "Non-finite binary float");
        Ok(v)
    }
    pub fn count(&mut self, max: usize) -> Result<usize> {
        let n = self.u32()? as usize;
        ensure!(n <= max, "Count {n} exceeds {max}");
        Ok(n)
    }
    pub fn remaining(&self) -> usize {
        self.data.len() - self.offset
    }
    pub fn position(&self) -> usize {
        self.offset
    }
    pub fn string8(&mut self) -> Result<String> {
        let len = self.u8()? as usize;
        Ok(std::str::from_utf8(self.bytes(len)?)?.to_owned())
    }
    pub fn blob32(&mut self, max: usize) -> Result<Vec<u8>> {
        let len = self.u32()? as usize;
        ensure!(len <= max, "Declared blob exceeds {max}-byte limit");
        Ok(self.bytes(len)?.to_vec())
    }
    pub fn finish(self) -> Result<()> {
        ensure!(
            self.offset == self.data.len(),
            "{} unparsed trailing bytes",
            self.data.len() - self.offset
        );
        Ok(())
    }
}
