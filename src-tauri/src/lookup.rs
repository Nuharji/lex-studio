//! Looking a word up: which Wiktionary pages to read for a pair of languages, and putting together what they say.
//!
//! - From German, or into German: the German Wiktionary (German meanings of words in every language; translation
//!   tables on German words).
//! - From English: the translation tables of the English Wiktionary.
//! - Any language: the English Wiktionary's meanings in English (for „→ English“ the translation itself).
//! - Nothing in the wanted language yet (e.g. Latin → French): through English — the English words of the meanings
//!   are looked up in the English Wiktionary's translation tables (marked „über Englisch: dog“).
//! An inflected form (canem) leads to its basic form (canis). A word not found exactly is searched loosely (λογος →
//! λόγος, ανθρωπος → ἄνθρωπος); otherwise similar words are offered.

use crate::lang;
use crate::parse::{self, Group};
use crate::wikitext::{self as wt, Seg};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Mutex;

/// Fetching a URL: Ok(None) when the page does not exist (404), Err when the network fails.
pub trait Net: Sync {
    fn get(&self, url: &str) -> Result<Option<String>, String>;
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Section {
    /// Which Wiktionary it comes from: "en" or "de".
    pub source: String,
    /// The page (the word as found; the basic form when `lemma_of` is set).
    pub title: String,
    pub pos: String,
    /// Meanings, in the language `mlang` ("en" or "de").
    pub meanings: Vec<Vec<Seg>>,
    pub mlang: String,
    pub translations: Vec<Group>,
    pub forms: Vec<[String; 2]>,
    /// Translations found through this English word.
    pub via: String,
    /// Set when this is the basic form of the word looked up (canis for canem).
    pub lemma_of: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Source {
    pub wiki: String,
    pub title: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Lookup {
    pub word: String,
    /// The page title the word was found under ("" = not found).
    pub title: String,
    pub from: String,
    pub to: String,
    pub sections: Vec<Section>,
    /// Similar words when nothing was found.
    pub suggestions: Vec<String>,
    pub sources: Vec<Source>,
    /// Some pages could not be fetched (no internet): the answer may be incomplete.
    pub partial: bool,
}

impl Lookup {
    pub fn found(&self) -> bool { !self.sections.is_empty() }
    /// Whether there is something in the target language (a translation or meanings written in it).
    pub fn has_target(&self) -> bool {
        self.sections.iter().any(|s| !s.translations.is_empty() || (s.mlang == self.to && !s.meanings.is_empty()))
    }
}

pub const UA: &str = concat!("LexStudio/", env!("CARGO_PKG_VERSION"), " (Studio family desktop dictionary)");

pub fn raw_url(wiki: &str, title: &str) -> String {
    format!("https://{wiki}.wiktionary.org/w/index.php?title={}&action=raw", wt::url_title(title))
}
pub fn rest_url(title: &str) -> String {
    format!("https://en.wiktionary.org/api/rest_v1/page/definition/{}", wt::url_title(title))
}
pub fn search_url(wiki: &str, q: &str, limit: usize) -> String {
    format!("https://{wiki}.wiktionary.org/w/api.php?action=opensearch&format=json&namespace=0&limit={limit}&profile=fuzzy&search={}", wt::url_query(q))
}
pub fn title_search_url(wiki: &str, q: &str) -> String {
    format!("https://{wiki}.wiktionary.org/w/api.php?action=query&list=search&format=json&srlimit=10&srprop=&srsearch={}", wt::url_query(&format!("intitle:{q}")))
}
pub fn page_url(wiki: &str, title: &str) -> String {
    format!("https://{wiki}.wiktionary.org/wiki/{}", wt::url_title(title))
}

/// The pages of one lookup, each fetched once; several at a time.
struct Pages<'a> {
    net: &'a dyn Net,
    memo: Mutex<HashMap<String, Result<Option<String>, String>>>,
}

impl<'a> Pages<'a> {
    fn new(net: &'a dyn Net) -> Self { Pages { net, memo: Mutex::new(HashMap::new()) } }

    fn get(&self, url: &str) -> Result<Option<String>, String> {
        if let Some(r) = self.memo.lock().unwrap().get(url) { return r.clone(); }
        let r = self.net.get(url);
        self.memo.lock().unwrap().insert(url.to_string(), r.clone());
        r
    }

    /// Fetches the ones not fetched yet at the same time.
    fn prefetch(&self, urls: &[String]) {
        let todo: Vec<&String> = { let m = self.memo.lock().unwrap(); urls.iter().filter(|u| !m.contains_key(*u)).collect() };
        if todo.len() < 2 { for u in todo { let _ = self.get(u); } return; }
        std::thread::scope(|s| { for u in todo { s.spawn(move || { let _ = self.get(u); }); } });
    }

    /// A wikitext page, following one redirect.
    fn raw(&self, wiki: &str, title: &str) -> Result<Option<(String, String)>, String> {
        let Some(p) = self.get(&raw_url(wiki, title))? else { return Ok(None) };
        if let Some(to) = parse::redirect(&p) {
            return Ok(self.get(&raw_url(wiki, &to))?.map(|p| (to, p)));
        }
        Ok(Some((title.to_string(), p)))
    }

    fn failed(&self) -> bool { self.memo.lock().unwrap().values().any(|r| r.is_err()) }
    fn all_failed(&self) -> bool { let m = self.memo.lock().unwrap(); !m.is_empty() && m.values().all(|r| r.is_err()) }
    /// Why nothing could be fetched: "rate-limit" (Wikimedia wants a pause) or "offline".
    fn why(&self) -> String {
        let m = self.memo.lock().unwrap();
        if m.values().any(|r| matches!(r, Err(e) if e == "rate-limit")) { "rate-limit".into() } else { "offline".into() }
    }
}

fn first_upper(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}
fn first_lower(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_lowercase().chain(c).collect()).unwrap_or_default()
}

/// The English name of a German part of speech, to pick matching translation tables („Substantiv, m“ → Noun).
fn pos_en(pos: &str) -> &str {
    let p = pos.split(',').next().unwrap_or("").trim();
    match p {
        "Substantiv" | "Nomen" => "Noun", "Eigenname" | "Vorname" | "Nachname" | "Toponym" => "Proper noun",
        "Verb" | "Hilfsverb" => "Verb", "Adjektiv" => "Adjective", "Adverb" => "Adverb", "Pronomen" | "Personalpronomen" => "Pronoun",
        "Präposition" => "Preposition", "Konjunktion" => "Conjunction", "Interjektion" => "Interjection", "Numerale" | "Kardinalzahl" => "Numeral",
        _ => p,
    }
}

pub fn lookup(net: &dyn Net, word: &str, from: &str, to: &str) -> Result<Lookup, String> {
    if lang::get(from).is_none() || lang::get(to).is_none() { return Err("language".into()); }
    let title = lang::title_form(word, from);
    if title.is_empty() { return Err("empty".into()); }
    let pages = Pages::new(net);
    let mut tried = Vec::new();
    // As typed, then with a capital / small first letter (German nouns, sentence starts).
    for t in [title.clone(), first_upper(&title), first_lower(&title)] {
        if tried.contains(&t) { continue; }
        tried.push(t.clone());
        let r = try_title(&pages, word, &t, from, to, true, true);
        if r.found() { return Ok(r); }
        if pages.all_failed() { return Err(pages.why()); }
    }
    // Loose search: the same word without accents / breathings (λογος → λόγος).
    let wiki = if from == "de" { "de" } else { "en" };
    // Prefix search and full-text title search (the latter ignores breathings: ανθρωπος → ἄνθρωπος), both at once.
    let (u1, u2) = (search_url(wiki, &title, 10), title_search_url(wiki, &title));
    pages.prefetch(&[u1.clone(), u2.clone()]);
    let json = |u: &str| pages.get(u).ok().flatten().and_then(|j| serde_json::from_str::<serde_json::Value>(&j).ok());
    let mut found: Vec<String> = json(&u2).and_then(|v| v.pointer("/query/search").cloned()).and_then(|v| v.as_array().cloned())
        .unwrap_or_default().iter().filter_map(|x| x.get("title").and_then(|t| t.as_str()).map(String::from)).collect();
    let prefix: Vec<String> = json(&u1).and_then(|v| v.get(1).cloned()).and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default();
    for p in prefix { if !found.contains(&p) { found.push(p); } }
    let (key, lkey) = (lang::fold(&title), lang::loose(&title));
    let exact = found.iter().filter(|c| lang::fold(c) == key);
    let loose = found.iter().filter(|c| lang::fold(c) != key && lang::loose(c) == lkey);
    for cand in exact.chain(loose).filter(|c| !tried.contains(c)).take(2) {
        let r = try_title(&pages, word, cand, from, to, true, true);
        if r.found() { return Ok(r); }
    }
    if pages.all_failed() { return Err(pages.why()); }
    Ok(Lookup { word: word.to_string(), from: from.into(), to: to.into(), suggestions: found.into_iter().filter(|c| !c.starts_with('-') && !c.ends_with('-')).take(8).collect(), partial: pages.failed(), ..Default::default() })
}

fn try_title(pages: &Pages, word: &str, title: &str, from: &str, to: &str, follow: bool, pivot: bool) -> Lookup {
    let lf = lang::get(from).unwrap();
    let use_de = from == "de" || to == "de";
    let mut urls = vec![rest_url(title)];
    if use_de { urls.push(raw_url("de", title)); }
    // English pages: their translation tables; Japanese ones: kana spellings point to the kanji words.
    if from == "en" || from == "ja" { urls.push(raw_url("en", title)); }
    pages.prefetch(&urls);

    let mut r = Lookup { word: word.to_string(), title: title.to_string(), from: from.into(), to: to.into(), ..Default::default() };
    let mut lemmas: Vec<String> = Vec::new();
    let mut only_forms = true;   // every entry is an inflected form
    let mut en_words: Vec<(String, String)> = Vec::new();   // (English word, part of speech) for going through English
    fn source(r: &mut Lookup, wiki: &str, t: &str) {
        if !r.sources.iter().any(|s| s.wiki == wiki && s.title == t) { r.sources.push(Source { wiki: wiki.into(), title: t.into(), url: page_url(wiki, t) }); }
    }

    // German Wiktionary: German meanings of the word (into German) or the translation tables of a German word.
    if use_de {
        if let Ok(Some((dt, page))) = pages.raw("de", title) {
            let target = if from == "de" { to } else { "de" };
            let entries = parse::de_entries(&page, lf.de, target);
            if !entries.is_empty() { source(&mut r, "de", &dt); }
            for e in entries {
                lemmas.extend(e.lemma.iter().cloned());
                only_forms &= e.form_only;
                if from == "de" {
                    // English translations of a German word lead to the other languages when needed.
                    if to != "en" {
                        for g in parse::de_entries(&page, lf.de, "en").into_iter().filter(|x| x.pos == e.pos).flat_map(|x| x.groups) {
                            for w in g.words { if !en_words.iter().any(|x| x.0 == w.w) { en_words.push((w.w, pos_en(&e.pos).to_string())); } }
                        }
                    }
                }
                let groups = if from == "de" && to != "de" { e.groups } else { vec![] };
                r.sections.push(Section { source: "de".into(), title: dt.clone(), pos: e.pos, meanings: e.meanings, mlang: "de".into(), translations: groups, forms: e.forms, via: String::new(), lemma_of: String::new() });
            }
        }
    }

    // English Wiktionary: meanings in English (any language).
    let defs = match pages.get(&rest_url(title)) { Ok(Some(j)) => parse::en_definitions(&j, from), _ => vec![] };
    if !defs.is_empty() { source(&mut r, "en", title); }
    // English word: the translation tables, by part of speech.
    let mut tables: Vec<(String, Group)> = Vec::new();
    if from == "en" && to != "en" {
        if let Ok(Some((et, page))) = pages.raw("en", title) {
            let (t, sub) = parse::en_translations(&page, to);
            tables = t;
            if !sub.is_empty() {
                if let Ok(Some((_, sp))) = pages.raw("en", &format!("{et}/translations")) { tables.extend(parse::en_translations(&sp, to).0); }
            }
            if !tables.is_empty() { source(&mut r, "en", &et); }
        }
    }
    for e in defs {
        lemmas.extend(e.lemma.iter().cloned());
        only_forms &= e.form_only;
        for w in &e.en_words { if !en_words.iter().any(|x| &x.0 == w) { en_words.push((w.clone(), e.pos.clone())); } }
        let mut groups: Vec<Group> = Vec::new();
        for (p, g) in tables.iter() { if p == &e.pos { groups.push(g.clone()); } }
        // In „→ German“ the German meanings are the translation; the English ones stay as extra help.
        r.sections.push(Section { source: "en".into(), title: title.to_string(), pos: e.pos, meanings: e.meanings, mlang: "en".into(), translations: groups, forms: vec![], via: String::new(), lemma_of: String::new() });
    }
    // Tables of a part of speech without English meanings (rare) still count.
    for (p, g) in tables.iter() {
        if !r.sections.iter().any(|s| s.source == "en" && &s.pos == p) {
            r.sections.push(Section { source: "en".into(), title: title.to_string(), pos: p.clone(), meanings: vec![], mlang: "en".into(), translations: vec![g.clone()], forms: vec![], via: String::new(), lemma_of: String::new() });
        }
    }

    // Nothing in the target language: through English (not when English is one of the two).
    if pivot && r.found() && !r.has_target() && from != "en" && to != "en" && from != to {
        // From German: the English translations of the word; else the English words the meanings link to.
        let cands: Vec<(String, String)> = en_words.iter().filter(|(w, _)| w.split(' ').count() <= 2).take(4).cloned().collect();
        pages.prefetch(&cands.iter().map(|(w, _)| raw_url("en", w)).collect::<Vec<_>>());
        for (w, pos) in cands {
            let Ok(Some((et, page))) = pages.raw("en", &w) else { continue };
            let (mut t, sub) = parse::en_translations(&page, to);
            if !sub.is_empty() {
                if let Ok(Some((_, sp))) = pages.raw("en", &format!("{et}/translations")) { t.extend(parse::en_translations(&sp, to).0); }
            }
            let same: Vec<Group> = t.iter().filter(|(p, _)| p == &pos).map(|(_, g)| g.clone()).collect();
            let groups: Vec<Group> = if same.is_empty() { t.into_iter().map(|(_, g)| g).collect() } else { same };
            let groups: Vec<Group> = groups.into_iter().take(2).collect();
            if groups.is_empty() || r.sections.iter().any(|s| s.via == et) { continue; }
            source(&mut r, "en", &et);
            r.sections.push(Section { source: "en".into(), title: title.to_string(), pos: pos.clone(), meanings: vec![], mlang: "en".into(), translations: groups, forms: vec![], via: et, lemma_of: String::new() });
        }
    }

    // A Japanese word in kana (いぬ) without meanings of its own: the kanji words its page points to (犬).
    if follow && from == "ja" && !r.found() {
        let see = match pages.raw("en", title) { Ok(Some((_, p))) => parse::ja_see(&p), _ => vec![] };
        pages.prefetch(&see.iter().take(2).map(|k| rest_url(k)).collect::<Vec<_>>());
        for k in see.iter().take(2) {
            let sub = try_title(pages, k, k, from, to, false, true);
            for mut s in sub.sections { s.lemma_of = title.to_string(); r.sections.push(s); }
            for s in sub.sources { source(&mut r, &s.wiki, &s.title); }
        }
    }

    // An inflected form (canem): its basic form (canis) too — when the word is only a form, or nothing else helped.
    let mut seen = Vec::new();
    lemmas.retain(|l| if seen.contains(l) || l == title { false } else { seen.push(l.clone()); true });
    if follow && r.found() && (only_forms || !r.has_target()) {
        for l in lemmas.iter().take(2) {
            let sub = try_title(pages, l, l, from, to, false, !r.has_target());
            for mut s in sub.sections { s.lemma_of = title.to_string(); r.sections.push(s); }
            for s in sub.sources { source(&mut r, &s.wiki, &s.title); }
        }
    }
    r.partial = pages.failed();
    r
}

/// Words starting like `prefix` (for the suggestions while typing).
pub fn suggest(net: &dyn Net, prefix: &str, from: &str) -> Result<Vec<String>, String> {
    let q = lang::title_form(prefix, from);
    if q.is_empty() { return Ok(vec![]); }
    let wiki = if from == "de" { "de" } else { "en" };
    let url = format!("https://{wiki}.wiktionary.org/w/api.php?action=opensearch&format=json&namespace=0&limit=8&search={}", wt::url_query(&q));
    let Some(j) = net.get(&url)? else { return Ok(vec![]) };
    let v: serde_json::Value = serde_json::from_str(&j).map_err(|e| e.to_string())?;
    Ok(v.get(1).and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|s| s.as_str().map(String::from)).filter(|s| !s.starts_with('-')).collect()).unwrap_or_default())
}

/* ---------------- The real network ---------------- */

pub struct Http(reqwest::blocking::Client);

impl Http {
    pub fn new() -> Self {
        let c = reqwest::blocking::Client::builder().user_agent(UA).timeout(std::time::Duration::from_secs(12))
            .connect_timeout(std::time::Duration::from_secs(6)).build().unwrap_or_default();
        Http(c)
    }
}

impl Net for Http {
    fn get(&self, url: &str) -> Result<Option<String>, String> {
        let r = self.0.get(url).send().map_err(|e| e.to_string())?;
        if r.status().as_u16() == 404 { return Ok(None); }
        // Wikimedia asks for a pause after many requests in a short time.
        if r.status().as_u16() == 429 { return Err("rate-limit".into()); }
        if !r.status().is_success() { return Err(format!("HTTP {}", r.status().as_u16())); }
        r.text().map(Some).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fixed pages instead of the network; counts the requests.
    struct Fake(HashMap<String, String>, Mutex<Vec<String>>, bool);
    impl Net for Fake {
        fn get(&self, url: &str) -> Result<Option<String>, String> {
            self.1.lock().unwrap().push(url.to_string());
            if self.2 { return Err("offline".into()); }
            Ok(self.0.get(url).cloned())
        }
    }
    fn fake(pages: &[(String, &str)]) -> Fake {
        Fake(pages.iter().map(|(u, p)| (u.clone(), p.to_string())).collect(), Mutex::new(vec![]), false)
    }

    const CANIS_REST: &str = r##"{"la":[{"partOfSpeech":"Noun","language":"Latin","definitions":[{"definition":"<a href=\"/wiki/dog#English\">dog</a>"}]}]}"##;
    const CANEM_REST: &str = r##"{"la":[{"partOfSpeech":"Noun","definitions":[{"definition":"<span class=\"form-of-definition\">accusative singular of <span class=\"form-of-definition-link\"><a href=\"/wiki/canis#Latin\">canis</a></span></span>"}]}]}"##;
    const DOG: &str = "==English==\n===Noun===\n====Translations====\n{{see translation subpage|Noun}}\n";
    const DOG_T: &str = "{{translation subpage}}\n==English==\n===Noun===\n====Translations====\n{{trans-top|animal}}{{multitrans|data=\n* French: {{tt+|fr|chien|m}}\n* Latin: {{tt+|la|canis|m}}\n}}\n{{trans-bottom}}\n";
    const CANIS_DE: &str = "== canis ({{Sprache|Latein}}) ==\n=== {{Wortart|Substantiv|Latein}}, {{mf}} ===\n{{Latein Substantiv Übersicht\n|Nominativ Singular=canis\n|Genitiv Singular=canis\n}}\n{{Bedeutungen}}\n:[1] {{K|klassischlateinisch}} [[Hund]], [[Hündin]]\n";

    #[test]
    fn latin_to_german_from_the_german_wiktionary() {
        let n = fake(&[(raw_url("de", "canis"), CANIS_DE), (rest_url("canis"), CANIS_REST)]);
        let r = lookup(&n, "canis", "la", "de").unwrap();
        assert!(r.found() && r.has_target());
        let de = r.sections.iter().find(|s| s.source == "de").unwrap();
        assert_eq!(wt::plain(&de.meanings[0]), "klassischlateinisch: Hund, Hündin");
        assert_eq!(de.forms.len(), 2);
        assert!(r.sections.iter().any(|s| s.mlang == "en"), "English meanings as extra help");
        assert_eq!(r.sources.len(), 2);
        assert!(!r.partial);
    }

    #[test]
    fn latin_to_french_through_english() {
        let n = fake(&[(rest_url("canis"), CANIS_REST), (raw_url("en", "dog"), DOG), (raw_url("en", "dog/translations"), DOG_T)]);
        let r = lookup(&n, "canis", "la", "fr").unwrap();
        let via = r.sections.iter().find(|s| s.via == "dog").expect("through English");
        assert_eq!(via.translations[0].words[0].w, "chien");
        assert_eq!(via.translations[0].gloss, "animal");
        assert!(r.has_target());
    }

    #[test]
    fn english_word_and_inflected_form() {
        let n = fake(&[(raw_url("en", "dog"), DOG), (raw_url("en", "dog/translations"), DOG_T), (rest_url("dog"), r##"{"en":[{"partOfSpeech":"Noun","definitions":[{"definition":"A mammal"}]}]}"##)]);
        let r = lookup(&n, "dog", "en", "la").unwrap();
        let s = r.sections.iter().find(|s| s.pos == "Noun").unwrap();
        assert_eq!(s.translations[0].words[0].w, "canis");
        assert_eq!(wt::plain(&s.meanings[0]), "A mammal");

        let n = fake(&[(rest_url("canem"), CANEM_REST), (rest_url("canis"), CANIS_REST)]);
        let r = lookup(&n, "canem", "la", "en").unwrap();
        assert!(r.sections.iter().any(|s| s.lemma_of == "canem" && s.title == "canis"), "{:?}", r.sections);
        assert!(r.has_target());
    }

    #[test]
    fn loose_search_capitals_and_offline() {
        let grc = r##"{"other":[{"partOfSpeech":"Noun","language":"Ancient Greek","definitions":[{"definition":"<a href=\"/wiki/word#English\">word</a>"}]}]}"##;
        let n = fake(&[(search_url("en", "λογος", 10), r#"["λογος",["-λογος"]]"#), (title_search_url("en", "λογος"), r#"{"query":{"search":[{"title":"λόγος"}]}}"#), (rest_url("λόγος"), grc)]);
        let r = lookup(&n, "λογος", "grc", "en").unwrap();
        assert_eq!(r.title, "λόγος");
        assert!(r.has_target());

        let hund = "== Hund ({{Sprache|Deutsch}}) ==\n=== {{Wortart|Substantiv|Deutsch}}, {{m}} ===\n{{Bedeutungen}}\n:[1] Tier\n==== {{Übersetzungen}} ====\n{{Ü-Tabelle|1|G=Tier|Ü-Liste=\n*{{la}}: {{Ü|la|canis}}\n}}\n";
        let n = fake(&[(raw_url("de", "Hund"), hund)]);
        let r = lookup(&n, "hund", "de", "la").unwrap();
        assert_eq!(r.title, "Hund");
        assert_eq!(r.sections[0].translations[0].words[0].w, "canis");

        let n = fake(&[(search_url("en", "xyzzy", 10), r#"["xyzzy",["xyz","xyzzy-"]]"#)]);
        let r = lookup(&n, "xyzzy", "la", "de").unwrap();
        assert!(!r.found());
        assert_eq!(r.suggestions, vec!["xyz"]);

        let off = Fake(HashMap::new(), Mutex::new(vec![]), true);
        assert_eq!(lookup(&off, "canis", "la", "de").unwrap_err(), "offline");
        assert!(lookup(&off, "  ", "la", "de").is_err());
        assert!(lookup(&off, "x", "la", "xx").is_err());
    }

    #[test]
    fn japanese_kana_leads_to_kanji() {
        let inu = r##"{"ja":[{"partOfSpeech":"Noun","language":"Japanese","definitions":[{"definition":"<a href=\"/wiki/dog#English\">dog</a>"}]}]}"##;
        let n = fake(&[(raw_url("en", "いぬ"), "==Japanese==\n{{ja-see|犬|狗}}\n"), (rest_url("犬"), inu)]);
        let r = lookup(&n, "いぬ", "ja", "en").unwrap();
        assert_eq!(r.title, "いぬ");
        let s = r.sections.iter().find(|s| s.title == "犬").expect("the kanji word");
        assert_eq!(s.lemma_of, "いぬ");
        assert_eq!(wt::plain(&s.meanings[0]), "dog");
        assert_eq!(parse::ja_see("{{ja-see|犬|狗|戌}} {{ja-see-kango|拳}}"), vec!["犬", "狗", "戌", "拳"]);
    }

    /// Against the real Wiktionary (needs the internet): cargo test -- --ignored
    #[test]
    #[ignore]
    fn live() {
        let h = Http::new();
        for (w, f, t, want) in [("canis", "la", "de", "Hund"), ("Hund", "de", "la", "canis"), ("dog", "en", "ja", "犬"), ("λόγος", "grc", "en", "word"),
            ("chien", "fr", "es", "perro"), ("собака", "ru", "de", "Hund"), ("cane", "it", "sv", "hund"), ("犬", "ja", "en", "dog"),
            ("perro", "es", "it", "cane"), ("hund", "sv", "fr", "chien"), ("canem", "la", "en", "dog"), ("ανθρωπος", "grc", "de", "Mensch"), ("amare", "la", "fr", "aimer"), ("いぬ", "ja", "de", "Hund"), ("ねこ", "ja", "en", "cat")] {
            let r = lookup(&h, w, f, t).unwrap();
            let all = serde_json::to_string(&r.sections).unwrap();
            println!("{w} {f}→{t}: {} sections, found {:?}", r.sections.len(), all.contains(want));
            assert!(all.contains(want), "{w} {f}→{t}: {want} missing in {all}");
        }
    }

    /// Shows what one lookup finds: LEX_Q="cane,it,sv" cargo test show -- --ignored --nocapture
    #[test]
    #[ignore]
    fn show() {
        let q = std::env::var("LEX_Q").unwrap_or_else(|_| "canem,la,de".into());
        let p: Vec<&str> = q.split(',').collect();
        let r = lookup(&Http::new(), p[0], p[1], p[2]).unwrap();
        println!("title {:?} suggestions {:?} partial {}", r.title, r.suggestions, r.partial);
        for s in &r.sections {
            println!("[{}] {} {} via={} lemma_of={} forms={}", s.source, s.title, s.pos, s.via, s.lemma_of, s.forms.len());
            for m in &s.meanings { println!("   - {}", wt::plain(m)); }
            for g in &s.translations { println!("   = {}: {:?}", g.gloss, g.words.iter().map(|w| &w.w).collect::<Vec<_>>()); }
        }
    }
}
