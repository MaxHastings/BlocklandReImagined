//! Text decoding for content originating in Windows editors.
use std::borrow::Cow;

/// Prefer UTF-8, falling back to Windows-1252 for an entire legacy file.
pub fn decode_utf8_or_windows_1252(bytes: &[u8]) -> Cow<'_, str> {
    std::str::from_utf8(bytes)
        .map_or_else(|_| Cow::Owned(decode_windows_1252(bytes)), Cow::Borrowed)
}

/// Windows-1252, the code page v20 wrote its text in, for 0x80..0xA0.
/// The five bytes it leaves undefined keep their Latin-1 control code.
const CP1252_HIGH: [char; 32] = [
    '\u{20ac}', '\u{0081}', '\u{201a}', '\u{0192}', '\u{201e}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{02c6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{008d}', '\u{017d}', '\u{008f}',
    '\u{0090}', '\u{2018}', '\u{2019}', '\u{201c}', '\u{201d}', '\u{2022}', '\u{2013}', '\u{2014}',
    '\u{02dc}', '\u{2122}', '\u{0161}', '\u{203a}', '\u{0153}', '\u{009d}', '\u{017e}', '\u{0178}',
];
pub fn decode_windows_1252(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&b| match b {
            0x80..0xa0 => CP1252_HIGH[usize::from(b - 0x80)],
            _ => char::from(b),
        })
        .collect()
}
