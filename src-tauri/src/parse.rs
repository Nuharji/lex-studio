//! Reading dictionary pages:
//! - English Wiktionary, wikitext: the translation tables of an English word (`{{trans-top|animal}}` … `{{t+|la|canis|m}}`);
//! - English Wiktionary, REST API (JSON): the meanings of a word in any language, in English;
//! - German Wiktionary, wikitext: meanings in German, the forms table (`{{Latein Substantiv Übersicht …}}`), the basic
//!   form of an inflected word and the translation tables of a German word (`{{Ü-Tabelle …}}` … `{{Ü|la|canis}}`).

use crate::lang;
use crate::wikitext::{self as wt, Seg, Tpl};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Word {
    pub w: String,
    /// Gender, transliteration, qualifier („m“, „いぬ, inu“, „formal“).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub note: String,
}

/// One translation table: the sense it is for and the words in the wanted language.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Group {
    pub gloss: String,
    pub words: Vec<Word>,
}

/// One part of speech of a word on a page.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Entry {
    pub pos: String,
    pub meanings: Vec<Vec<Seg>>,
    pub groups: Vec<Group>,
    pub forms: Vec<[String; 2]>,
    /// The basic form when this is an inflected form (canem → canis).
    pub lemma: Vec<String>,
    /// English words the meanings link to (for looking up translations through English).
    pub en_words: Vec<String>,
    /// Only an inflected form („accusative singular of canis“), no meaning of its own.
    pub form_only: bool,
}

const POS_EN: [&str; 24] = ["Noun", "Proper noun", "Verb", "Adjective", "Adverb", "Pronoun", "Preposition", "Postposition",
    "Conjunction", "Interjection", "Numeral", "Article", "Determiner", "Particle", "Phrase", "Prepositional phrase",
    "Proverb", "Idiom", "Prefix", "Suffix", "Contraction", "Participle", "Letter", "Symbol"];

/// The level-2 section of a language (`==Latin==` … up to the next `==…==`).
fn en_section<'a>(page: &'a str, heading: &str) -> Option<&'a str> {
    let mut start = None;
    let mut pos = 0;
    for line in page.split_inclusive('\n') {
        let l = line.trim_end();
        let level2 = l.starts_with("==") && !l.starts_with("===") && l.ends_with("==");
        if level2 {
            if start.is_some() { return Some(&page[start.unwrap()..pos]); }
            if l.trim_matches('=').trim() == heading { start = Some(pos + line.len()); }
        }
        pos += line.len();
    }
    start.map(|s| &page[s..])
}

fn heading(line: &str) -> Option<&str> {
    let l = line.trim();
    (l.starts_with("==") && l.ends_with("==")).then(|| l.trim_matches('=').trim())
}

/// A word from a translation template, cleaned (`[[an]] [[den]] Fersen` → „an den Fersen“).
fn clean_word(s: &str) -> String {
    wt::plain(&wt::wiki_segs(s))
}

const T_TEMPLATES: [&str; 8] = ["t", "t+", "tt", "tt+", "t-check", "t+check", "t-simple", "tt-check"];
const GENDERS: [&str; 14] = ["m", "f", "n", "c", "p", "s", "d", "m-p", "f-p", "n-p", "m-s", "f-s", "impf", "pf"];

/// The translations into `target` on one line of a translation table (`* German: {{tt+|de|Hund|m}}, …`).
fn en_line_words(line: &str, target: &str) -> Vec<Word> {
    let mut out: Vec<Word> = Vec::new();
    let mut last_ours = false;
    for t in wt::templates(line) {
        if T_TEMPLATES.contains(&t.name.as_str()) {
            last_ours = t.p(0) == target;
            if !last_ours { continue; }
            let w = clean_word(t.get("alt").unwrap_or(t.p(1)));
            if w.is_empty() { last_ours = false; continue; }
            let mut note: Vec<String> = t.pos.iter().skip(2).filter(|g| GENDERS.contains(&g.as_str())).cloned().collect();
            if let Some(tr) = t.get("tr").filter(|s| !s.is_empty()) { note.push(tr.to_string()); }
            if let Some(lit) = t.get("lit").filter(|s| !s.is_empty()) { note.push(format!("„{lit}“")); }
            if !out.iter().any(|x| x.w == w) { out.push(Word { w, note: note.join(" · ") }); }
        } else if last_ours && matches!(t.name.as_str(), "q" | "qualifier" | "i" | "qual" | "gloss" | "gl") {
            if let Some(last) = out.last_mut() {
                let q = t.pos.join(", ");
                last.note = if last.note.is_empty() { q } else { format!("{} · {q}", last.note) };
            }
        } else {
            last_ours = false;
        }
    }
    out
}

/// The translation tables of the English section of an en.wiktionary page, with the words in `target`; groups without
/// such words are left out. Also says for which parts of speech the tables are on a subpage (`word/translations`).
pub fn en_translations(page: &str, target: &str) -> (Vec<(String, Group)>, Vec<String>) {
    let Some(sec) = en_section(page, "English").or_else(|| page.contains("{{translation subpage}}").then_some(page)) else { return (vec![], vec![]) };
    let (mut out, mut sub) = (Vec::new(), Vec::new());
    let mut pos = String::new();
    let mut group: Option<Group> = None;
    for line in sec.lines() {
        if let Some(h) = heading(line) {
            if POS_EN.contains(&h) { pos = h.to_string(); }
            continue;
        }
        for t in wt::templates(line) {
            match t.name.as_str() {
                "trans-top" | "checktrans-top" | "trans-top-also" => {
                    let gloss = if t.name == "checktrans-top" { String::new() } else { clean_word(t.p(0)) };
                    group = Some(Group { gloss, words: vec![] });
                }
                "trans-bottom" => {
                    if let Some(g) = group.take().filter(|g| !g.words.is_empty()) { out.push((pos.clone(), g)); }
                }
                "see translation subpage" => {
                    let p = if t.p(0).is_empty() { pos.clone() } else { t.p(0).to_string() };
                    if !sub.contains(&p) { sub.push(p); }
                }
                _ => {}
            }
        }
        if let Some(g) = group.as_mut() {
            if line.starts_with('*') {
                for w in en_line_words(line, target) {
                    if !g.words.iter().any(|x| x.w == w.w) { g.words.push(w); }
                }
            }
        }
    }
    if let Some(g) = group.filter(|g| !g.words.is_empty()) { out.push((pos, g)); }
    (out, sub)
}

/// The meanings of a word in one language from the REST answer (`/api/rest_v1/page/definition/…`), in English.
pub fn en_definitions(json: &str, lang_code: &str) -> Vec<Entry> {
    let Ok(v) = serde_json::from_str::<Value>(json) else { return vec![] };
    let Some(l) = lang::get(lang_code) else { return vec![] };
    let mut items: Vec<&Value> = v.get(lang_code).and_then(Value::as_array).map(|a| a.iter().collect()).unwrap_or_default();
    // Languages without their own key (Ancient Greek) are under "other" with their name.
    if let Some(other) = v.get("other").and_then(Value::as_array) {
        items.extend(other.iter().filter(|i| i.get("language").and_then(Value::as_str) == Some(l.en)));
    }
    let mut out = Vec::new();
    for it in items {
        let mut e = Entry { pos: it.get("partOfSpeech").and_then(Value::as_str).unwrap_or("").to_string(), form_only: true, ..Default::default() };
        let mut links: Vec<Vec<String>> = Vec::new();
        for d in it.get("definitions").and_then(Value::as_array).into_iter().flatten() {
            let html = d.get("definition").and_then(Value::as_str).unwrap_or("");
            let segs = wt::html_segs(html);
            if wt::plain(&segs).trim().is_empty() { continue; }
            if html.contains("form-of-definition") {
                // canem: "accusative singular of canis" — the last link inside the form-of link is the basic form.
                if let Some(part) = html.split("form-of-definition-link").nth(1) {
                    if let Some((page, _)) = wt::html_links(part).into_iter().next() {
                        if !e.lemma.contains(&page) { e.lemma.push(page); }
                    }
                }
            } else {
                e.form_only = false;
                links.push(wt::html_links(html).into_iter().filter(|(_, a)| a.starts_with("English") || a.is_empty()).map(|(p, _)| p).collect::<Vec<_>>());
            }
            e.meanings.push(segs);
        }
        // The English words for going through English: first the first word of every meaning, then the second …
        for i in 0..4 {
            for l in &links {
                if let Some(p) = l.get(i) { if !e.en_words.contains(p) && e.en_words.len() < 12 { e.en_words.push(p.clone()); } }
            }
        }
        if !e.meanings.is_empty() { out.push(e); }
    }
    out
}

/* ---------------- German Wiktionary ---------------- */

/// Text for a template inside a German meaning: labels like `{{K|Bergbau}}` → „Bergbau: “, abbreviations like
/// `{{ugs.}}` → „ugs. “, genders, translations; everything else (quality notes, references) goes.
fn de_tpl_text(t: &Tpl) -> String {
    let n = t.name.as_str();
    match n {
        "K" => {
            let labels: Vec<&str> = t.pos.iter().map(String::as_str).filter(|s| !s.is_empty()).collect();
            if labels.is_empty() { String::new() } else { format!("{}: ", labels.join(", ")) }
        }
        "Ü" | "Üt" | "L" | "lang" => t.p(1).to_string(),
        "m" | "f" | "n" | "mf" | "fm" | "u" | "c" | "Pl." | "Sg." => format!("{n} "),
        "Wortart" => t.p(0).to_string(),
        _ if n.ends_with('.') && !n.contains(' ') => {
            let p = t.p(0);
            if p.chars().all(|c| c.is_ascii_punctuation()) { format!("{n}{p} ") } else { format!("{n} ") }
        }
        _ => String::new(),
    }
}

fn de_text(s: &str) -> Vec<Seg> {
    wt::wiki_segs(&wt::expand(&wt::strip_notes(s), &de_tpl_text))
}

/// The language name in a German heading: `== canis ({{Sprache|Latein}}) ==` → „Latein“.
fn de_lang(line: &str) -> Option<String> {
    wt::templates(line).into_iter().find(|t| t.name == "Sprache").map(|t| t.p(0).to_string())
}

/// The entries of a de.wiktionary page in one language (`de_name`, e.g. „Latein“), with the translation tables into
/// `target` (only on German pages there are tables into other languages).
pub fn de_entries(page: &str, de_name: &str, target: &str) -> Vec<Entry> {
    let mut out: Vec<Entry> = Vec::new();
    let (mut in_lang, mut cur): (bool, Option<Entry>) = (false, None);
    let (mut block, mut forms_open, mut table_open, mut gloss) = (String::new(), false, false, String::new());
    let finish = |cur: &mut Option<Entry>, out: &mut Vec<Entry>| {
        if let Some(mut e) = cur.take() {
            e.form_only = !e.lemma.is_empty() && e.pos.contains("Form");
            if !e.meanings.is_empty() || !e.groups.is_empty() || !e.lemma.is_empty() || !e.forms.is_empty() { out.push(e); }
        }
    };
    for raw in page.lines() {
        let line = raw.trim_end();
        let t = line.trim_start();
        if t.starts_with("== ") || (t.starts_with("==") && !t.starts_with("===")) {
            finish(&mut cur, &mut out);
            in_lang = de_lang(t).as_deref() == Some(de_name);
            continue;
        }
        if !in_lang { continue; }
        if t.starts_with("===") && !t.starts_with("====") {
            finish(&mut cur, &mut out);
            let pos = wt::plain(&de_text(t.trim_matches('=').trim()));
            cur = Some(Entry { pos: pos.trim().trim_end_matches(',').to_string(), ..Default::default() });
            block.clear();
            continue;
        }
        let Some(e) = cur.as_mut() else { continue };
        // The forms table: `{{Latein Substantiv Übersicht` … `}}`, one `|key=value` per line.
        if forms_open {
            if t == "}}" { forms_open = false; continue; }
            if let Some((k, v)) = t.trim_start_matches('|').split_once('=') {
                let k = k.trim().replace('_', " ");
                let v = wt::plain(&de_text(&v.replace("<br />", ", ").replace("<br/>", ", ").replace("<br>", ", ")));
                let skip = k.contains("Bild") || k.contains("Genus") || k.starts_with("Weitere") || k.contains("Hilfsverb")
                    || k.starts_with("Stamm") || k.starts_with("kein") || k.starts_with("Keine") || k.starts_with("Unpersönlich");
                if !skip && !v.trim().is_empty() && v.trim() != "—" && v.trim() != "-" && e.forms.len() < 48 {
                    e.forms.push([k, v.trim().to_string()]);
                }
            }
            continue;
        }
        if t.starts_with("{{") && t.contains("Übersicht") && !t.contains("}}") {
            forms_open = true;
            continue;
        }
        // Translation table: `{{Ü-Tabelle|1|G=…|Ü-Liste=` then `*{{la}}: {{Ü|la|canis}}` … until the dialect part or `}}`.
        if t.starts_with("{{Ü-Tabelle") {
            table_open = true;
            gloss = t.split("G=").nth(1).map(|g| g.split('|').next().unwrap_or("").to_string()).unwrap_or_default();
            gloss = wt::plain(&de_text(&gloss));
            continue;
        }
        if table_open {
            if t.starts_with("|Dialekttabelle") || t == "}}" { table_open = false; continue; }
            if t.starts_with('*') {
                let words = de_line_words(t, target);
                if !words.is_empty() {
                    match e.groups.iter_mut().find(|g| g.gloss == gloss) {
                        Some(g) => for w in words { if !g.words.iter().any(|x| x.w == w.w) { g.words.push(w); } },
                        None => e.groups.push(Group { gloss: gloss.clone(), words }),
                    }
                }
            }
            continue;
        }
        // Basic form of an inflected word.
        for tp in wt::templates(t) {
            if tp.name.starts_with("Grundformverweis") && !tp.p(0).is_empty() && !e.lemma.contains(&tp.pos[0]) {
                e.lemma.push(tp.pos[0].clone());
            }
        }
        // A block starts with a lone template line: {{Bedeutungen}}, {{Beispiele}} …
        if t.starts_with("{{") && t.ends_with("}}") && wt::templates(t).len() == 1 && wt::templates(t)[0].end == t.len() {
            block = wt::templates(t)[0].name.clone();
            continue;
        }
        if t.starts_with("====") { block.clear(); continue; }
        match block.as_str() {
            "Bedeutungen" if t.starts_with(":[") || (t.starts_with(':') && !t.starts_with("::")) => {
                let text = t.trim_start_matches(':');
                let text = if text.starts_with('[') { text.split_once(']').map(|x| x.1).unwrap_or(text) } else { text };
                let segs = de_text(text);
                if !wt::plain(&segs).is_empty() { e.meanings.push(segs); }
            }
            "Grammatische Merkmale" if t.starts_with('*') => {
                let segs = de_text(t.trim_start_matches('*'));
                if !wt::plain(&segs).is_empty() { e.meanings.push(segs); }
            }
            _ => {}
        }
    }
    finish(&mut cur, &mut out);
    out
}

/// The translations into `target` on one line of a German translation table (`*{{la}}: {{Ü|la|canis}} {{m}}, …`).
fn de_line_words(line: &str, target: &str) -> Vec<Word> {
    let mut out: Vec<Word> = Vec::new();
    let mut last_ours = false;
    for t in wt::templates(line) {
        let n = t.name.as_str();
        if n == "Ü" || n == "Üt" || n.starts_with("Üxx") {
            last_ours = t.p(0) == target;
            if !last_ours { continue; }
            let w = clean_word(t.get("v").unwrap_or(t.p(1)));
            if w.is_empty() { last_ours = false; continue; }
            let note = if n == "Üt" { clean_word(t.p(2)) } else { String::new() };
            if !out.iter().any(|x| x.w == w) { out.push(Word { w, note }); }
        } else if last_ours && matches!(n, "m" | "f" | "n" | "u" | "c" | "Pl." | "mf" | "fm") {
            if let Some(last) = out.last_mut() {
                last.note = if last.note.is_empty() { n.to_string() } else { format!("{} · {n}", last.note) };
            }
        } else if last_ours && n.ends_with('.') {
            if let Some(last) = out.last_mut() {
                last.note = if last.note.is_empty() { n.to_string() } else { format!("{} · {n}", last.note) };
            }
        } else {
            last_ours = false;
        }
    }
    out
}

/// The kanji words a Japanese kana page points to (`{{ja-see|犬|狗}}` on いぬ): such pages have no meanings of their own.
pub fn ja_see(page: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in wt::templates(page).into_iter().filter(|t| t.name.starts_with("ja-see")) {
        for p in t.pos.iter().map(|p| p.trim()).filter(|p| !p.is_empty()) {
            if !out.iter().any(|o| o == p) { out.push(p.to_string()); }
        }
    }
    out
}

/// A redirect page (`#REDIRECT [[x]]` / `#WEITERLEITUNG [[x]]`) → its target.
pub fn redirect(page: &str) -> Option<String> {
    let first = page.trim_start().lines().next()?;
    let up = first.to_uppercase();
    if !(up.starts_with("#REDIRECT") || up.starts_with("#WEITERLEITUNG")) { return None; }
    let t = first.split("[[").nth(1)?.split("]]").next()?;
    Some(t.split('#').next()?.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOG: &str = "==English==\n===Etymology===\nx\n===Noun===\n{{en-noun}}\n# animal\n====Translations====\n\
{{trans-top|animal}}\n* Ancient Greek: {{t|grc|κύων|m|f}}\n* German: {{t+|de|Hund|m}}, {{t+|de|Hündin|f}} {{q|female}}\n\
* Japanese: {{t+|ja|犬|tr=いぬ, inu}}\n* Latin: {{t+|la|canis|m|f}}\n{{trans-bottom}}\n{{trans-top|man}}\n* French: {{t|fr|type}}\n{{trans-bottom}}\n\
===Verb===\n====Translations====\n{{trans-top|pursue}}{{multitrans|data=\n* German: {{tt+|de|jagen}}, {{tt+|de|verfolgen}}\n}}\n{{trans-bottom}}\n\
{{see translation subpage|Proper noun}}\n==French==\n===Noun===\n{{trans-top|x}}\n* German: {{t|de|falsch}}\n{{trans-bottom}}\n";

    #[test]
    fn english_translation_tables() {
        let (g, sub) = en_translations(DOG, "de");
        assert_eq!(g.len(), 2, "the French table has no German, the French section is not English: {g:?}");
        assert_eq!(g[0].0, "Noun");
        assert_eq!(g[0].1.gloss, "animal");
        assert_eq!(g[0].1.words, vec![Word { w: "Hund".into(), note: "m".into() }, Word { w: "Hündin".into(), note: "f · female".into() }]);
        assert_eq!(g[1].0, "Verb");
        assert_eq!(g[1].1.words.len(), 2);
        assert_eq!(sub, vec!["Proper noun"]);
        let (g, _) = en_translations(DOG, "ja");
        assert_eq!(g[0].1.words[0], Word { w: "犬".into(), note: "いぬ, inu".into() });
        let (g, _) = en_translations(DOG, "grc");
        assert_eq!(g[0].1.words[0], Word { w: "κύων".into(), note: "m · f".into() });
    }

    #[test]
    fn rest_definitions() {
        let j = r##"{"la":[{"partOfSpeech":"Noun","language":"Latin","definitions":[{"definition":"<a href=\"/wiki/dog#English\">dog</a>, <a href=\"/wiki/hound#English\">hound</a>"},{"definition":""}]}],
          "other":[{"partOfSpeech":"Noun","language":"Ancient Greek","definitions":[{"definition":"<a rel=\"mw:WikiLink\" href=\"/wiki/word#English\">word</a>, speech"}]},
                   {"partOfSpeech":"Noun","language":"Greek","definitions":[{"definition":"modern"}]}]}"##;
        let la = en_definitions(j, "la");
        assert_eq!(la.len(), 1);
        assert_eq!(la[0].meanings.len(), 1);
        assert_eq!(wt::plain(&la[0].meanings[0]), "dog, hound");
        assert_eq!(la[0].en_words, vec!["dog", "hound"]);
        let grc = en_definitions(j, "grc");
        assert_eq!(grc.len(), 1, "only Ancient Greek, not Greek");
        assert_eq!(wt::plain(&grc[0].meanings[0]), "word, speech");
        let form = r##"{"la":[{"partOfSpeech":"Noun","definitions":[{"definition":"<span class=\"form-of-definition use-with-mention\"><a href=\"/wiki/Appendix:Glossary#accusative_case\">accusative</a> singular of <span class=\"form-of-definition-link\"><i><a href=\"/wiki/canis#Latin\">canis</a></i></span></span>"}]}]}"##;
        let e = en_definitions(form, "la");
        assert_eq!(e[0].lemma, vec!["canis"]);
        assert!(e[0].en_words.is_empty());
        assert!(en_definitions("kaputt", "la").is_empty());
    }

    const HUND: &str = "== Hund ({{Sprache|Deutsch}}) ==\n=== {{Wortart|Substantiv|Deutsch}}, {{m}} ===\n\n{{Deutsch Substantiv Übersicht\n|Genus=m\n|Nominativ Singular=Hund\n|Nominativ Plural=Hunde\n|Bild=x.jpg\n}}\n\n\
{{Bedeutungen}}\n:[1] [[Haustier]], dessen Vorfahre der [[Wolf]] ist\n:[2] {{K|Schimpfwort}} [[gemein]]er [[Mitmensch]]\n\n{{Synonyme}}\n:[1] [[Köter]]\n\n\
==== {{Übersetzungen}} ====\n{{Ü-Tabelle|1|G=Haustier, dessen Vorfahre der Wolf ist|Ü-Liste=\n*{{grc}}: {{Üt|grc|κύων}} {{m}}\n*{{en}}: {{Ü|en|dog}}, {{Ü|en|hound}}\n*{{la}}: {{Ü|la|canis}} {{m}}\n|Dialekttabelle=\n*Bairisch: {{Ü|la|falsch}}\n}}\n\
== hund ({{Sprache|Schwedisch}}) ==\n=== {{Wortart|Substantiv|Schwedisch}}, {{u}} ===\n{{Bedeutungen}}\n:[1] [[Hund]]\n";

    #[test]
    fn german_pages() {
        let e = de_entries(HUND, "Deutsch", "la");
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].pos, "Substantiv, m");
        assert_eq!(e[0].meanings.len(), 2, "synonyms are not meanings");
        assert_eq!(wt::plain(&e[0].meanings[1]), "Schimpfwort: gemeiner Mitmensch");
        assert_eq!(e[0].forms, vec![["Nominativ Singular".to_string(), "Hund".to_string()], ["Nominativ Plural".to_string(), "Hunde".to_string()]]);
        assert_eq!(e[0].groups.len(), 1);
        assert_eq!(e[0].groups[0].gloss, "Haustier, dessen Vorfahre der Wolf ist");
        assert_eq!(e[0].groups[0].words, vec![Word { w: "canis".into(), note: "m".into() }], "not the dialect table");
        let en = de_entries(HUND, "Deutsch", "en");
        assert_eq!(en[0].groups[0].words.len(), 2);
        let sv = de_entries(HUND, "Schwedisch", "de");
        assert_eq!(sv[0].pos, "Substantiv, u");
        assert_eq!(wt::plain(&sv[0].meanings[0]), "Hund");
        let canem = "== canem ({{Sprache|Latein}}) ==\n=== {{Wortart|Deklinierte Form|Latein}} ===\n\n{{Grammatische Merkmale}}\n*Akkusativ Singular des Substantivs '''[[canis]]'''\n\n{{Grundformverweis Dekl|canis|spr=la}}";
        let c = de_entries(canem, "Latein", "de");
        assert_eq!(c[0].lemma, vec!["canis"]);
        assert_eq!(wt::plain(&c[0].meanings[0]), "Akkusativ Singular des Substantivs canis");
        assert_eq!(redirect("#WEITERLEITUNG [[Hund#x]]"), Some("Hund".into()));
        assert_eq!(redirect("== x =="), None);
    }
}
