//! What Lex Studio offers over MCP (mcp.rs): looking words up in the same ten languages as the window, from Wiktionary.

use crate::{lang, lookup, mcp, wikitext};
use serde_json::{json, Value};

pub fn run() { mcp::serve("lex-studio", env!("CARGO_PKG_VERSION"), &tools(), &call) }

fn codes() -> String { lang::LANGS.iter().map(|l| format!("{} ({})", l.code, l.en)).collect::<Vec<_>>().join(", ") }

fn tools() -> Vec<Value> {
    let l = |d: &str| json!({"type": "string", "description": format!("{d}: {}", codes())});
    vec![
        mcp::tool("lookup", "Look a word up in the dictionary (Wiktionary): translations into the target language, meanings, forms; an inflected form (canem) also gives its basic form (canis). Needs the internet.",
            json!({"word": {"type": "string"}, "from": l("Language of the word (may be left out for Greek, Cyrillic or Japanese script)"), "to": l("Language to translate into")}), &["word", "to"]),
        mcp::tool("suggest", "Dictionary words starting with some letters (for a word you are not sure how to spell).",
            json!({"prefix": {"type": "string"}, "from": l("Language")}), &["prefix", "from"]),
        mcp::tool("languages", "The languages Lex Studio knows (code and name).", json!({}), &[]),
    ]
}

/// The lookup as short readable text (what an AI client needs), with the sources at the end.
fn summary(r: &lookup::Lookup) -> String {
    if !r.found() {
        let s = if r.suggestions.is_empty() { String::new() } else { format!(" Similar: {}.", r.suggestions.join(", ")) };
        return format!("„{}“ was not found.{s}", r.word);
    }
    let mut out = format!("{} ({} → {})\n", r.title, r.from, r.to);
    for s in &r.sections {
        out.push_str(&format!("\n[{}{}{}]\n", s.pos,
            if s.lemma_of.is_empty() { String::new() } else { format!(", basic form of {}", s.lemma_of) },
            if s.via.is_empty() { String::new() } else { format!(", through English „{}“", s.via) }));
        if !s.meanings.is_empty() {
            out.push_str(&format!("Meanings ({}):\n", s.mlang));
            for (i, m) in s.meanings.iter().enumerate() { out.push_str(&format!("  {}. {}\n", i + 1, wikitext::plain(m))); }
        }
        for g in &s.translations {
            let words: Vec<String> = g.words.iter().map(|w| if w.note.is_empty() { w.w.clone() } else { format!("{} ({})", w.w, w.note) }).collect();
            out.push_str(&format!("Translation{}: {}\n", if g.gloss.is_empty() { String::new() } else { format!(" „{}“", g.gloss) }, words.join(", ")));
        }
        if !s.forms.is_empty() {
            out.push_str(&format!("Forms: {}\n", s.forms.iter().map(|f| format!("{} {}", f[0], f[1])).collect::<Vec<_>>().join("; ")));
        }
    }
    out.push_str(&format!("\nSource: Wiktionary (CC BY-SA 4.0) — {}", r.sources.iter().map(|s| s.url.as_str()).collect::<Vec<_>>().join(", ")));
    out
}

fn call(name: &str, args: &Value) -> Result<Value, String> {
    let code = |k: &str| -> Result<String, String> {
        let c = mcp::text(args, k)?;
        lang::get(&c).map(|l| l.code.to_string()).ok_or_else(|| format!("{k}: unknown language „{c}“ — use one of {}", codes()))
    };
    match name {
        "languages" => Ok(json!(lang::LANGS.iter().map(|l| json!({"code": l.code, "name": l.en, "de": l.de})).collect::<Vec<_>>())),
        "lookup" => {
            let word = mcp::text(args, "word")?;
            // Without „from“ the script tells the language (λόγος → grc, собака → ru, 犬 → ja).
            let from = match args.get("from").and_then(Value::as_str).filter(|s| !s.is_empty()) {
                Some(_) => code("from")?,
                None => lang::script_of(&word).map(String::from).ok_or("from: needed for words in Latin letters")?,
            };
            let r = lookup::lookup(&lookup::Http::new(), &word, &from, &code("to")?)
                .map_err(|e| match e.as_str() { "offline" => "No internet connection (Wiktionary could not be reached).".to_string(), "rate-limit" => "Wiktionary asks for a short pause (too many requests) — try again in a minute.".to_string(), _ => e })?;
            Ok(Value::String(summary(&r)))
        }
        "suggest" => Ok(json!(lookup::suggest(&lookup::Http::new(), &mcp::text(args, "prefix")?, &code("from")?)?)),
        _ => Err(format!("unknown tool: {name}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tools_answer_without_network() {
        assert_eq!(call("languages", &json!({})).unwrap().as_array().unwrap().len(), 10);
        assert!(call("lookup", &json!({"word": "x", "from": "xx", "to": "de"})).unwrap_err().contains("unknown language"));
        assert!(call("nope", &json!({})).is_err());
        assert!(call("lookup", &json!({"word": "canis", "to": "de"})).unwrap_err().contains("from: needed"));
        let r = lookup::Lookup { word: "x".into(), suggestions: vec!["xy".into()], ..Default::default() };
        assert!(summary(&r).contains("Similar: xy"));
        assert_eq!(tools().len(), 3);
    }
}
