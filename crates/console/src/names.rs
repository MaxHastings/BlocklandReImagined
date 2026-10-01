//! Which characters a player name or clan tag may hold, shared by the name
//! boxes (what typing accepts) and the host (what it keeps).
//!
//! v20's boxes took the Windows-1252 characters its font caches hold. Beyond
//! those, a name may use any script and symbol the UI draws correctly one
//! character at a time (Greek, Cyrillic, Armenian, Georgian, Chinese,
//! Japanese, Korean, arrows, shapes, emoji...): the UI draws glyphs the v20
//! caches lack from the system's fonts (`bri_ui::fallback`). Left out:
//! scripts that need joining or reordering (Arabic, Hebrew, Indic, Thai...),
//! combining marks (stacked "Zalgo" text), invisible characters (zero-width,
//! bidi controls, soft hyphen, blank fillers) and multi-character emoji
//! (skin tones, flags), which would draw as loose pieces.

/// `c` as a name keeps it: `None` if names may not hold it, and a plain
/// space for the no-break and ideographic spaces, so no two names differ by
/// a space nobody can tell apart.
pub fn name_char(c: char) -> Option<char> {
    let u = c as u32;
    match u {
        0xA0 | 0x3000 => return Some(' '),
        // Soft hyphen, blank Braille, Hangul fillers: drawn as nothing.
        0xAD | 0x2800 | 0x3164 | 0xFFA0 => return None,
        _ => {}
    }
    let kept = matches!(
        u,
        0x20..=0x7E
            // Latin-1, Latin Extended-A/B, IPA
            | 0xA1..=0x2AF
            // Greek and Coptic
            | 0x370..=0x377
            | 0x37A..=0x37F
            | 0x384..=0x38A
            | 0x38C
            | 0x38E..=0x3A1
            | 0x3A3..=0x3FF
            // Cyrillic and its supplement, without the combining signs
            | 0x400..=0x482
            | 0x48A..=0x52F
            // Armenian, Georgian
            | 0x531..=0x556
            | 0x559..=0x58A
            | 0x10A0..=0x10FF
            // Latin Extended Additional, Greek Extended
            | 0x1E00..=0x1FFE
            // Dashes, quotes, bullets (no separators or bidi controls)
            | 0x2010..=0x2027
            | 0x2030..=0x205E
            // Super/subscripts, currency
            | 0x2070..=0x2071
            | 0x2074..=0x208E
            | 0x2090..=0x209C
            | 0x20A0..=0x20C0
            // Letterlike, arrows, maths, technical, shapes, symbols, dingbats
            | 0x2100..=0x2BFF
            // CJK radicals, punctuation, kana, bopomofo, Hangul, ideographs
            | 0x2E80..=0x2FDF
            | 0x3001..=0x3029
            | 0x3030..=0x303F
            | 0x3041..=0x3096
            | 0x309B..=0x30FF
            | 0x3105..=0x312F
            | 0x3131..=0x318E
            | 0x31A0..=0x31FF
            | 0x3200..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0xAC00..=0xD7A3
            | 0xF900..=0xFAFF
            // Full- and halfwidth forms
            | 0xFF01..=0xFFDC
            | 0xFFE0..=0xFFEE
            // Emoji and other pictographs, without flag letters and skin tones
            | 0x1F000..=0x1F1E5
            | 0x1F200..=0x1F3FA
            | 0x1F400..=0x1FAFF
    );
    kept.then_some(c)
}

/// What a name reads as, for telling whether two names can pass for each
/// other: case folded, fullwidth letters as ASCII, and Cyrillic and Greek
/// letters that look like Latin ones (`Мах`, `Μax`) as those Latin
/// letters, like Unicode's confusable skeletons (UTS #39) for the scripts
/// names mix most. `I`, `l`, `1` and `|` read as one letter, and `0` as
/// `o`.
pub fn skeleton(name: &str) -> String {
    name.trim()
        .chars()
        .filter_map(name_char)
        .map(|c| match c as u32 {
            0xFF01..=0xFF5E => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
            _ => c,
        })
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'l' | '1' | '|' | 'ӏ' | 'ı' | 'ℓ' | 'і' | 'ї' | 'ι' => 'i',
            '0' | 'о' | 'ο' => 'o',
            'а' | 'α' => 'a',
            'в' | 'β' => 'b',
            'с' | 'ϲ' => 'c',
            'ԁ' => 'd',
            'е' | 'ё' | 'ε' => 'e',
            'ɡ' => 'g',
            'н' | 'һ' => 'h',
            'ј' | 'ϳ' => 'j',
            'к' | 'κ' => 'k',
            'м' | 'μ' => 'm',
            'η' => 'n',
            'р' | 'ρ' => 'p',
            'ԛ' => 'q',
            'ѕ' => 's',
            'т' | 'τ' => 't',
            'υ' => 'u',
            'ν' => 'v',
            'ԝ' | 'ω' => 'w',
            'х' | 'χ' | '×' => 'x',
            'у' | 'ү' | 'γ' => 'y',
            'ζ' => 'z',
            c => c,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_and_symbols_the_ui_draws_are_kept() {
        for c in "Aé©™€ΩЖԱაあア中한★♥☺→∞😀🎮".chars() {
            assert_eq!(name_char(c), Some(c), "{c:?} U+{:04X}", c as u32);
        }
    }

    #[test]
    fn invisible_joined_and_combining_characters_are_not() {
        for c in [
            '\u{0}', '\n', '\u{7F}', '\u{85}', '\u{AD}', '\u{200B}', '\u{200D}', '\u{202E}',
            '\u{2060}', '\u{FEFF}', '\u{301}', '\u{489}', '\u{FE0F}', '\u{2800}', '\u{3164}',
            '\u{FFA0}', '\u{E000}', 'ب', 'ש', 'क', 'ก', '\u{1F3FD}', '\u{1F1FA}',
        ] {
            assert_eq!(name_char(c), None, "U+{:04X}", c as u32);
        }
        assert_eq!(name_char('\u{A0}'), Some(' '));
        assert_eq!(name_char('\u{3000}'), Some(' '));
    }

    #[test]
    fn lookalike_names_share_a_skeleton() {
        let max = skeleton("Max");
        for lookalike in ["МАХ", "Мах", "Μax", "ＭＡＸ", "max", " Max "] {
            assert_eq!(skeleton(lookalike), max, "{lookalike}");
        }
        assert_eq!(skeleton("Bill"), skeleton("BiII"));
        assert_eq!(skeleton("B0b"), skeleton("Bob"));
        assert_ne!(skeleton("Max"), skeleton("Mäx"));
        assert_ne!(skeleton("Max"), skeleton("Maxwell"));
    }
}
