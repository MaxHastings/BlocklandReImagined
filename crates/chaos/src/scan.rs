//! Finds NaN and infinity anywhere in a value, by encoding it the way the
//! wire does (MessagePack keeps every float as a float) and walking the
//! bytes. Reports where each one sits, as a path of map keys and indices.
use anyhow::{Context, Result, bail};
use rmp::Marker;
use serde::Serialize;

/// Paths of every non-finite float in `value`, at most `limit` of them.
pub fn non_finite<T: Serialize + ?Sized>(value: &T, limit: usize) -> Result<Vec<String>> {
    let bytes = rmp_serde::to_vec_named(value).context("Encode for scanning")?;
    let mut scan = Scan {
        bytes: &bytes,
        at: 0,
        path: Vec::new(),
        found: Vec::new(),
        limit,
    };
    scan.value()?;
    anyhow::ensure!(scan.at == bytes.len(), "Scanner left trailing bytes");
    Ok(scan.found)
}

/// Fails naming the first non-finite floats in `value`.
pub fn ensure_finite<T: Serialize + ?Sized>(what: &str, value: &T) -> Result<()> {
    let found = non_finite(value, 8)?;
    anyhow::ensure!(
        found.is_empty(),
        "{what} holds non-finite floats at {}",
        found.join(", ")
    );
    Ok(())
}

struct Scan<'a> {
    bytes: &'a [u8],
    at: usize,
    path: Vec<String>,
    found: Vec<String>,
    limit: usize,
}
impl Scan<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8]> {
        let end = self.at.checked_add(n).context("Length overflow")?;
        let slice = self.bytes.get(self.at..end).context("Truncated value")?;
        self.at = end;
        Ok(slice)
    }
    fn uint(&mut self, n: usize) -> Result<u64> {
        Ok(self.take(n)?.iter().fold(0, |v, b| v << 8 | u64::from(*b)))
    }
    fn report(&mut self, value: f64) {
        if !value.is_finite() && self.found.len() < self.limit {
            let path = if self.path.is_empty() {
                "<root>".into()
            } else {
                self.path.join(".")
            };
            self.found.push(format!("{path} = {value}"));
        }
    }
    /// A map key, kept readable for the path when it is a string or number.
    fn key(&mut self) -> Result<String> {
        let start = self.at;
        let marker = Marker::from_u8(self.take(1)?[0]);
        let text = match marker {
            Marker::FixStr(n) => Some(n as usize),
            Marker::Str8 => Some(self.uint(1)? as usize),
            Marker::Str16 => Some(self.uint(2)? as usize),
            Marker::Str32 => Some(self.uint(4)? as usize),
            _ => None,
        };
        if let Some(n) = text {
            return Ok(String::from_utf8_lossy(self.take(n)?).into_owned());
        }
        if let Marker::FixPos(n) = marker {
            return Ok(n.to_string());
        }
        self.at = start;
        self.path.push("<key>".into());
        self.value()?;
        self.path.pop();
        Ok("<key>".into())
    }
    fn value(&mut self) -> Result<()> {
        let marker = Marker::from_u8(self.take(1)?[0]);
        let (items, pairs) = match marker {
            Marker::Null | Marker::True | Marker::False | Marker::FixPos(_) | Marker::FixNeg(_) => {
                return Ok(());
            }
            Marker::U8 | Marker::I8 => return self.take(1).map(drop),
            Marker::U16 | Marker::I16 => return self.take(2).map(drop),
            Marker::U32 | Marker::I32 => return self.take(4).map(drop),
            Marker::U64 | Marker::I64 => return self.take(8).map(drop),
            Marker::F32 => {
                let bits = self.uint(4)? as u32;
                self.report(f64::from(f32::from_bits(bits)));
                return Ok(());
            }
            Marker::F64 => {
                let bits = self.uint(8)?;
                self.report(f64::from_bits(bits));
                return Ok(());
            }
            Marker::FixStr(n) => return self.take(n as usize).map(drop),
            Marker::Str8 | Marker::Bin8 => (self.uint(1)?, false),
            Marker::Str16 | Marker::Bin16 => (self.uint(2)?, false),
            Marker::Str32 | Marker::Bin32 => (self.uint(4)?, false),
            Marker::FixArray(n) => (u64::from(n), true),
            Marker::Array16 => (self.uint(2)?, true),
            Marker::Array32 => (self.uint(4)?, true),
            Marker::FixMap(n) => return self.map(u64::from(n)),
            Marker::Map16 => {
                let n = self.uint(2)?;
                return self.map(n);
            }
            Marker::Map32 => {
                let n = self.uint(4)?;
                return self.map(n);
            }
            Marker::FixExt1 => return self.take(2).map(drop),
            Marker::FixExt2 => return self.take(3).map(drop),
            Marker::FixExt4 => return self.take(5).map(drop),
            Marker::FixExt8 => return self.take(9).map(drop),
            Marker::FixExt16 => return self.take(17).map(drop),
            Marker::Ext8 => (self.uint(1)? + 1, false),
            Marker::Ext16 => (self.uint(2)? + 1, false),
            Marker::Ext32 => (self.uint(4)? + 1, false),
            Marker::Reserved => bail!("Reserved MessagePack marker"),
        };
        if !pairs {
            return self.take(items as usize).map(drop);
        }
        for i in 0..items {
            self.path.push(i.to_string());
            self.value()?;
            self.path.pop();
        }
        Ok(())
    }
    fn map(&mut self, n: u64) -> Result<()> {
        for _ in 0..n {
            let key = self.key()?;
            self.path.push(key);
            self.value()?;
            self.path.pop();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[derive(Serialize)]
    struct Pose {
        name: String,
        position: [f32; 3],
        extra: Option<Vec<f64>>,
        bytes: serde_bytes_like::Bytes,
    }
    mod serde_bytes_like {
        #[derive(serde::Serialize)]
        pub struct Bytes(pub Vec<u8>);
    }

    #[test]
    fn finds_nan_and_infinity_with_their_paths() {
        let mut map = BTreeMap::new();
        map.insert(
            7_u64,
            Pose {
                name: "nan in a string is fine".into(),
                position: [0.0, f32::NAN, 1.0],
                extra: Some(vec![1.0, f64::INFINITY]),
                bytes: serde_bytes_like::Bytes(vec![0xca, 0x7f, 0xc0, 0, 0]),
            },
        );
        let found = non_finite(&map, 8).unwrap();
        assert_eq!(found, ["7.position.1 = NaN", "7.extra.1 = inf"]);
        map.get_mut(&7).unwrap().position[1] = 2.0;
        map.get_mut(&7).unwrap().extra = None;
        assert!(non_finite(&map, 8).unwrap().is_empty());
    }
}
