//! The languages Lex Studio knows: their code (the one Wiktionary uses in translation tables), the heading of their
//! section on the English Wiktionary and the name on the German one. Plus the text helpers every lookup needs:
//! dropping marks that page titles leave out (Latin macrons, Russian stress) and folding a word for loose comparison.

use unicode_normalization::UnicodeNormalization;

pub struct Lang {
    pub code: &'static str,
    /// Section heading on en.wiktionary.org (`==Latin==`).
    pub en: &'static str,
    /// Language name on de.wiktionary.org (`{{Sprache|Latein}}`).
    pub de: &'static str,
}

pub const LANGS: [Lang; 10] = [
    Lang { code: "la", en: "Latin", de: "Latein" },
    Lang { code: "grc", en: "Ancient Greek", de: "Altgriechisch" },
    Lang { code: "en", en: "English", de: "Englisch" },
    Lang { code: "de", en: "German", de: "Deutsch" },
    Lang { code: "fr", en: "French", de: "Französisch" },
    Lang { code: "es", en: "Spanish", de: "Spanisch" },
    Lang { code: "ru", en: "Russian", de: "Russisch" },
    Lang { code: "it", en: "Italian", de: "Italienisch" },
    Lang { code: "sv", en: "Swedish", de: "Schwedisch" },
    Lang { code: "ja", en: "Japanese", de: "Japanisch" },
];

pub fn get(code: &str) -> Option<&'static Lang> { LANGS.iter().find(|l| l.code == code) }

/// The word as the page title has it: trimmed, single spaces; Latin without macrons / breves (canī → cani), Russian
/// without stress marks (соба́ка → собака). Ancient Greek keeps its accents (they are part of the title).
pub fn title_form(word: &str, code: &str) -> String {
    let w = word.split_whitespace().collect::<Vec<_>>().join(" ");
    match code {
        "la" => w.nfd().filter(|c| !matches!(*c, '\u{0304}' | '\u{0306}')).nfc().collect(),
        "ru" => w.nfd().filter(|c| !matches!(*c, '\u{0301}' | '\u{0300}')).nfc().collect(),
        _ => w,
    }
}

/// Loose form for comparing: lower case, without accents, breathings, macrons; final sigma as sigma.
pub fn fold(s: &str) -> String {
    s.nfd().filter(|c| !('\u{0300}'..='\u{036f}').contains(c)).flat_map(char::to_lowercase)
        .map(|c| if c == 'ς' { 'σ' } else { c }).collect()
}

/// Even looser, for words typed with Latin letters: ο/ω and ε/η count as the same (anthropos → ανθροπος finds ἄνθρωπος).
pub fn loose(s: &str) -> String {
    fold(s).chars().map(|c| match c { 'ω' => 'ο', 'η' => 'ε', _ => c }).collect()
}

/// Which of our languages a word is written in, when its script tells (Greek, Cyrillic, Japanese); None for Latin script.
pub fn script_of(word: &str) -> Option<&'static str> {
    for c in word.chars() {
        match c as u32 {
            0x0370..=0x03ff | 0x1f00..=0x1fff => return Some("grc"),
            0x0400..=0x04ff => return Some("ru"),
            0x3040..=0x30ff | 0x3400..=0x4dbf | 0x4e00..=0x9fff | 0xff66..=0xff9f => return Some("ja"),
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_and_folding() {
        assert_eq!(title_form("  canī  ", "la"), "cani");
        assert_eq!(title_form("amō amās", "la"), "amo amas");
        assert_eq!(title_form("соба́ка", "ru"), "собака");
        assert_eq!(title_form("λόγος", "grc"), "λόγος");
        assert_eq!(fold("ἄνθρωπος"), "ανθρωποσ");
        assert_eq!(fold("Λόγος"), fold("λογος"));
        assert_eq!(loose("ἄνθρωπος"), loose("ανθροπος"));
        assert_eq!(script_of("λόγος"), Some("grc"));
        assert_eq!(script_of("собака"), Some("ru"));
        assert_eq!(script_of("犬"), Some("ja"));
        assert_eq!(script_of("いぬ"), Some("ja"));
        assert_eq!(script_of("Hund"), None);
        assert!(LANGS.iter().all(|l| get(l.code).is_some()));
    }
}
