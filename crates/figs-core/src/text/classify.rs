//! Script classes of characters, for per-script font specs
//! (`font_family = { latin = "…", cjk = "…" }`, see
//! [`crate::schema::FontSpec`]).
//!
//! Modelled on how Office assigns a character to a run's Latin or East Asian
//! font: by Unicode block, with the "ambiguous" symbol and punctuation blocks
//! (General Punctuation, arrows, math operators, geometric shapes, the Latin-1
//! symbols `× ÷ ° ± §` …) going to the East Asian font when the text is
//! Japanese / Chinese / Korean (in Office: the run's language or the IME it
//! was typed with) and to the Latin font otherwise. figs has no language
//! tags, so an ambiguous character **follows its neighbours**: the nearest
//! preceding letter, digit or CJK character decides (the following one at
//! the start of a paragraph; CJK when there is none). A `symbol` slot, when
//! given, takes them all instead.
//!
//! | Class | Characters |
//! |---|---|
//! | Latin | ASCII (letters, digits, space, punctuation), Latin-1 letters, Latin Extended, IPA, spacing modifiers, Greek, Cyrillic, other Unicode spaces (U+2000–200A, U+202F, U+205F), U+00A0 |
//! | CJK | Hangul, CJK radicals/symbols/punctuation (`、。「」〜` U+3000 ideographic space), kana (`・ ー` included), Bopomofo, CJK ideographs (all planes), Yi, compatibility and vertical forms, half-/full-width forms (`（）！０Ａ･`) |
//! | Ambiguous | Latin-1 symbols `¡–¿` (but not U+00A0) and `× ÷`, General Punctuation `‐ – — ‘ ’ “ ” † ‡ • … ‰ ′ ″ ※`, super/subscripts, currency, letterlike `℃ №`, number forms `Ⅰ`, arrows, math operators `− ± ∞`, technical, enclosed alphanumerics `①`, box drawing, block elements, geometric shapes `○ ■ △`, misc symbols `★ ♪`, dingbats |
//! | Other | everything else (Arabic, Hebrew, Indic, Thai, emoji, private use, …) |
//!
//! Combining marks, variation selectors and joiners are not classified: they
//! stay in the face of the character they attach to.

/// The script class of a character (see the module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharClass {
    Latin,
    Cjk,
    /// Symbol / punctuation shared by Latin and CJK text: follows its
    /// neighbours unless the spec has a `symbol` slot.
    Ambiguous,
    Other,
}

/// Which slot list a character uses, after resolving ambiguous characters by
/// context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Slot {
    Latin,
    Cjk,
    /// An ambiguous character; `cjk` = its context is CJK text.
    Symbol {
        cjk: bool,
    },
    Other,
}

/// The script class of `ch`.
pub fn char_class(ch: char) -> CharClass {
    let c = ch as u32;
    match c {
        // ASCII: letters, digits, space and punctuation are all Latin.
        0x0000..=0x007F => CharClass::Latin,
        0x00A0 => CharClass::Latin,
        0x00A1..=0x00BF | 0x00D7 | 0x00F7 => CharClass::Ambiguous,
        // Latin-1 letters, Latin Extended A/B, IPA, spacing modifiers.
        0x00C0..=0x02FF => CharClass::Latin,
        // Combining diacritics (normally attached; Latin when standalone).
        0x0300..=0x036F => CharClass::Latin,
        // Greek, Cyrillic (+ supplement).
        0x0370..=0x052F => CharClass::Latin,
        0x1100..=0x11FF => CharClass::Cjk,
        0x1C80..=0x1C8F | 0x1D00..=0x1DBF => CharClass::Latin,
        // Latin Extended Additional, Greek Extended.
        0x1E00..=0x1FFF => CharClass::Latin,
        // Spaces of General Punctuation.
        0x2000..=0x200A | 0x202F | 0x205F => CharClass::Latin,
        0x200B..=0x200F | 0x2028..=0x202E | 0x2060..=0x206F => CharClass::Other,
        // Punctuation and symbol blocks up to Misc Symbols and Arrows.
        0x2010..=0x2027 | 0x2030..=0x205E | 0x2070..=0x2BFF => CharClass::Ambiguous,
        0x2C60..=0x2C7F | 0x2DE0..=0x2DFF => CharClass::Latin,
        // CJK radicals .. CJK compatibility, Ext A, URO, Yi.
        0x2E80..=0xA4CF => CharClass::Cjk,
        0xA640..=0xA69F | 0xA720..=0xA7FF | 0xAB30..=0xAB6F => CharClass::Latin,
        0xA960..=0xA97F | 0xAC00..=0xD7FF | 0xF900..=0xFAFF => CharClass::Cjk,
        0xFB00..=0xFB06 => CharClass::Latin,
        // Vertical forms, CJK compatibility / small forms, half/full width.
        0xFE10..=0xFE1F | 0xFE30..=0xFE6F | 0xFF00..=0xFFEF => CharClass::Cjk,
        // Kana supplements, enclosed ideographic supplement, SIP and beyond.
        0x1AFF0..=0x1B16F | 0x1F200..=0x1F2FF | 0x20000..=0x3FFFF => CharClass::Cjk,
        _ => CharClass::Other,
    }
}

/// Whether `ch` is a strong context character: a Latin letter or digit, or
/// any CJK character (`Some(true)`), as opposed to spaces, punctuation and
/// symbols, which never decide an ambiguous neighbour.
fn strong(ch: char) -> Option<bool> {
    match char_class(ch) {
        CharClass::Latin if ch.is_alphanumeric() => Some(false),
        CharClass::Cjk => Some(true),
        _ => None,
    }
}

/// The slot of every character of `s` (in `char_indices` order), ambiguous
/// characters resolved by the nearest strong character before them, else
/// after them, else CJK.
pub(crate) fn slots(s: &str) -> Vec<Slot> {
    let chars: Vec<char> = s.chars().collect();
    let mut before: Vec<Option<bool>> = Vec::with_capacity(chars.len());
    let mut last = None;
    for &c in &chars {
        before.push(last);
        if let Some(k) = strong(c) {
            last = Some(k);
        }
    }
    let mut after: Vec<Option<bool>> = vec![None; chars.len()];
    let mut next = None;
    for (i, &c) in chars.iter().enumerate().rev() {
        after[i] = next;
        if let Some(k) = strong(c) {
            next = Some(k);
        }
    }
    chars
        .iter()
        .enumerate()
        .map(|(i, &c)| match char_class(c) {
            CharClass::Latin => Slot::Latin,
            CharClass::Cjk => Slot::Cjk,
            CharClass::Other => Slot::Other,
            CharClass::Ambiguous => Slot::Symbol {
                cjk: before[i].or(after[i]).unwrap_or(true),
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classes() {
        for c in ['A', 'z', '0', ' ', '(', '-', 'é', 'Δ', 'μ', 'Ж', '\u{00A0}'] {
            assert_eq!(char_class(c), CharClass::Latin, "{c:?}");
        }
        for c in [
            'あ', 'ア', '・', 'ー', '漢', '、', '。', '「', '\u{3000}', '（', '･', '한', '𠮷',
        ] {
            assert_eq!(char_class(c), CharClass::Cjk, "{c:?}");
        }
        for c in [
            '×', '÷', '°', '±', '—', '–', '“', '…', '※', '−', '→', '○', '■', '①', '℃', '★',
        ] {
            assert_eq!(char_class(c), CharClass::Ambiguous, "{c:?}");
        }
        for c in ['ا', 'א', 'ก', '😀', '\u{E000}'] {
            assert_eq!(char_class(c), CharClass::Other, "{c:?}");
        }
    }

    #[test]
    fn ambiguous_follow_neighbours() {
        let s = slots("5 × 10、○佐藤 — x");
        let sym = |i: usize| match s[i] {
            Slot::Symbol { cjk } => cjk,
            other => panic!("{other:?}"),
        };
        assert!(!sym(2)); // × after the digit 5
        assert!(sym(7)); // ○ after 、 (CJK)
        assert!(sym(11)); // — after 藤
        assert_eq!(s[0], Slot::Latin);
        assert_eq!(s[1], Slot::Latin); // space
                                       // At the start: the following strong character decides.
        assert_eq!(slots("■ Results")[0], Slot::Symbol { cjk: false });
        assert_eq!(slots("■ 結果")[0], Slot::Symbol { cjk: true });
        // No strong character at all: CJK.
        assert_eq!(slots("→")[0], Slot::Symbol { cjk: true });
    }
}
