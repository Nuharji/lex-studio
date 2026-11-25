//! Just enough wikitext and HTML to read dictionary pages: templates (`{{t+|de|Hund|m}}`) with their parameters,
//! links, and turning a line of wikitext or a bit of HTML into text pieces where links stay clickable.

use serde::Serialize;

/// One template: `{{name|a|b|key=value}}` → name, positional [a, b], named [(key, value)].
#[derive(Debug, Clone, PartialEq)]
pub struct Tpl {
    pub name: String,
    pub pos: Vec<String>,
    pub named: Vec<(String, String)>,
    /// Byte range in the text it came from.
    pub start: usize,
    pub end: usize,
}

impl Tpl {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.named.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }
    pub fn p(&self, i: usize) -> &str { self.pos.get(i).map(String::as_str).unwrap_or("") }
}

/// A piece of text; `l` is the page it links to ("" = plain text).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Seg {
    pub t: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub l: String,
}

/// Where the `}}` closing the template that starts at `start` (pointing at `{{`) ends; None when it never closes.
fn close(s: &str, start: usize) -> Option<usize> {
    let b = s.as_bytes();
    let (mut i, mut depth) = (start, 0usize);
    while i + 1 < b.len() {
        if b[i] == b'{' && b[i + 1] == b'{' { depth += 1; i += 2; continue; }
        if b[i] == b'}' && b[i + 1] == b'}' {
            depth -= 1;
            i += 2;
            if depth == 0 { return Some(i); }
            continue;
        }
        i += 1;
    }
    None
}

/// Splits at `|` outside nested templates and links.
fn split_params(inner: &str) -> Vec<String> {
    let b = inner.as_bytes();
    let (mut out, mut cur, mut tpl, mut link, mut i) = (Vec::new(), 0usize, 0i32, 0i32, 0usize);
    while i < b.len() {
        let two = |c: u8| i + 1 < b.len() && b[i] == c && b[i + 1] == c;
        if two(b'{') { tpl += 1; i += 2; continue; }
        if two(b'}') { tpl -= 1; i += 2; continue; }
        if two(b'[') { link += 1; i += 2; continue; }
        if two(b']') { link -= 1; i += 2; continue; }
        if b[i] == b'|' && tpl <= 0 && link <= 0 {
            out.push(inner[cur..i].to_string());
            cur = i + 1;
        }
        i += 1;
    }
    out.push(inner[cur..].to_string());
    out
}

fn parse_at(s: &str, start: usize) -> Option<Tpl> {
    let end = close(s, start)?;
    let mut parts = split_params(&s[start + 2..end - 2]).into_iter();
    let name = parts.next()?.trim().to_string();
    let (mut pos, mut named) = (Vec::new(), Vec::new());
    for p in parts {
        // `key=value` only when the key is a plain word (a `=` inside a link or template is part of the value).
        match p.split_once('=') {
            Some((k, v)) if !k.is_empty() && !k.contains(['{', '[', '<']) => named.push((k.trim().to_string(), v.trim().to_string())),
            _ => pos.push(p.trim().to_string()),
        }
    }
    Some(Tpl { name, pos, named, start, end })
}

/// The top-level templates of a text, in order. A `{{` that never closes (a table spanning lines) is skipped.
pub fn templates(s: &str) -> Vec<Tpl> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(off) = s[i..].find("{{") {
        let at = i + off;
        match parse_at(s, at) {
            Some(t) => { i = t.end; out.push(t); }
            None => i = at + 2,
        }
    }
    out
}

/// Removes `<!-- comments -->` and `<ref>…</ref>` (with what is inside).
pub fn strip_notes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    loop {
        let c = rest.find("<!--");
        let r = rest.find("<ref");
        let next = match (c, r) { (Some(a), Some(b)) => Some(a.min(b)), (a, b) => a.or(b) };
        let Some(at) = next else { out.push_str(rest); break };
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        rest = if tail.starts_with("<!--") {
            tail.find("-->").map(|e| &tail[e + 3..]).unwrap_or("")
        } else {
            let open_end = tail.find('>').unwrap_or(tail.len() - 1);
            if tail[..=open_end].ends_with("/>") { &tail[open_end + 1..] }
            else { tail.find("</ref>").map(|e| &tail[e + 6..]).unwrap_or("") }
        };
    }
    out
}

/// Replaces every template by the text `f` gives for it (recursively for templates inside that text).
pub fn expand(s: &str, f: &dyn Fn(&Tpl) -> String) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last = 0;
    for t in templates(s) {
        out.push_str(&s[last..t.start]);
        let x = f(&t);
        out.push_str(&if x.contains("{{") { expand(&x, f) } else { x });
        last = t.end;
    }
    out.push_str(&s[last..]);
    out
}

/// Decodes the HTML entities that occur in Wiktionary text.
pub fn entities(s: &str) -> String {
    if !s.contains('&') { return s.to_string(); }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        let semi = tail[..tail.len().min(12)].find(';');
        let (rep, used) = match semi {
            Some(e) => {
                let name = &tail[1..e];
                let c = match name {
                    "amp" => Some('&'), "lt" => Some('<'), "gt" => Some('>'), "quot" => Some('"'), "apos" | "#39" => Some('\''),
                    "nbsp" | "#160" => Some(' '), "ndash" => Some('–'), "mdash" => Some('—'), "thinsp" => Some(' '),
                    _ if name.starts_with("#x") => u32::from_str_radix(&name[2..], 16).ok().and_then(char::from_u32),
                    _ if name.starts_with('#') => name[1..].parse().ok().and_then(char::from_u32),
                    _ => None,
                };
                match c { Some(c) => (c.to_string(), e + 1), None => ("&".into(), 1) }
            }
            None => ("&".into(), 1),
        };
        out.push_str(&rep);
        rest = &tail[used..];
    }
    out.push_str(rest);
    out
}

/// Drops HTML tags (keeps what is between them).
fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut inside = false;
    for c in s.chars() {
        match c {
            '<' => inside = true,
            '>' if inside => inside = false,
            _ if !inside => out.push(c),
            _ => {}
        }
    }
    out
}

fn push(segs: &mut Vec<Seg>, t: &str, l: &str) {
    if t.is_empty() { return; }
    if l.is_empty() {
        if let Some(last) = segs.last_mut().filter(|s| s.l.is_empty()) { last.t.push_str(t); return; }
    }
    segs.push(Seg { t: t.to_string(), l: l.to_string() });
}

/// Tidies the pieces: single spaces, no space before punctuation, trimmed ends, no empty pieces.
pub fn tidy(segs: Vec<Seg>) -> Vec<Seg> {
    let mut out: Vec<Seg> = Vec::new();
    for s in segs {
        let t = s.t.split_whitespace().collect::<Vec<_>>().join(" ");
        let lead = s.t.starts_with(char::is_whitespace) && !t.is_empty();
        let trail = s.t.ends_with(char::is_whitespace) && !t.is_empty();
        let t = format!("{}{}{}", if lead { " " } else { "" }, t, if trail { " " } else { "" });
        // A piece of only spaces between two others stays one space.
        if t.is_empty() { if !s.t.is_empty() && s.l.is_empty() { push(&mut out, " ", ""); } continue; }
        push(&mut out, &t, &s.l);
    }
    for i in 0..out.len() {
        if i + 1 < out.len() && out[i].t.ends_with(' ') && out[i + 1].t.starts_with(' ') { out[i].t.pop(); }
    }
    if let Some(f) = out.first_mut() { f.t = f.t.trim_start().to_string(); }
    if let Some(l) = out.last_mut() { l.t = l.t.trim_end().to_string(); }
    out.retain(|s| !s.t.is_empty());
    for s in out.iter_mut() {
        for p in [" ,", " .", " ;", " :", " )"] { s.t = s.t.replace(p, &p[1..]); }
        s.t = s.t.replace("( ", "(");
    }
    out
}

/// Plain text of segments.
pub fn plain(segs: &[Seg]) -> String { segs.iter().map(|s| s.t.as_str()).collect() }

/// Wikitext (templates already expanded) → pieces: `[[target|label]]` becomes a link, files and categories go,
/// `''` / `'''` and HTML tags go.
pub fn wiki_segs(s: &str) -> Vec<Seg> {
    let s = strip_tags(&strip_notes(s)).replace("'''", "").replace("''", "");
    let s = entities(&s);
    let mut segs = Vec::new();
    let mut rest = s.as_str();
    while let Some(at) = rest.find("[[") {
        push(&mut segs, &rest[..at], "");
        let tail = &rest[at + 2..];
        let Some(e) = tail.find("]]") else { push(&mut segs, &rest[at..], ""); rest = ""; break };
        let inner = &tail[..e];
        let (target, label) = inner.split_once('|').unwrap_or((inner, inner));
        let target = target.trim();
        let lower = target.to_lowercase();
        let skip = ["datei:", "file:", "bild:", "image:", "kategorie:", "category:"].iter().any(|p| lower.starts_with(p));
        if !skip {
            // Other namespaces (Wikipedia `w:`, Appendix:, Verzeichnis:) stay plain text.
            let page = target.split('#').next().unwrap_or("").trim();
            let link = if page.contains(':') || page.is_empty() { "" } else { page };
            push(&mut segs, label, link);
        }
        rest = &tail[e + 2..];
    }
    push(&mut segs, rest, "");
    tidy(segs)
}

/// Percent-decodes a URL path piece.
pub fn url_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) { out.push(v); i += 3; continue; }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Percent-encodes a page title for a URL (spaces as `_`, like MediaWiki).
pub fn url_title(t: &str) -> String {
    let mut out = String::new();
    for b in t.replace(' ', "_").bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { out.push(b as char); } else { out.push_str(&format!("%{b:02X}")); }
    }
    out
}

/// Percent-encodes a query value.
pub fn url_query(t: &str) -> String {
    let mut out = String::new();
    for b in t.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { out.push(b as char); } else { out.push_str(&format!("%{b:02X}")); }
    }
    out
}

/// HTML from the Wiktionary REST API → pieces; a link to a dictionary page (`/wiki/canis#Latin`) stays a link, links
/// to other namespaces (Appendix:Glossary) become plain text.
pub fn html_segs(html: &str) -> Vec<Seg> {
    let mut segs = Vec::new();
    let mut rest = html;
    let mut link: Option<String> = None;
    while let Some(at) = rest.find('<') {
        push(&mut segs, &entities(&rest[..at]), link.as_deref().unwrap_or(""));
        let tail = &rest[at..];
        let Some(e) = tail.find('>') else { rest = ""; break };
        let tag = &tail[..=e];
        // Inline CSS / scripts (Wiktionary puts <style> blocks into some definitions) are not text.
        if let Some(close) = ["style", "script"].iter().find(|n| tag.starts_with(&format!("<{n}")) && !tag.ends_with("/>")).map(|n| format!("</{n}>")) {
            rest = tail.find(&close).map(|c| &tail[c + close.len()..]).unwrap_or("");
            continue;
        }
        if tag.starts_with("<a ") || tag == "<a>" {
            link = Some(href_page(tag).unwrap_or_default());
        } else if tag == "</a>" || tag.starts_with("</a ") {
            link = None;
        }
        rest = &tail[e + 1..];
    }
    push(&mut segs, &entities(rest), link.as_deref().unwrap_or(""));
    tidy(segs)
}

/// The dictionary page an `<a href="/wiki/…">` points to; None for other namespaces and outside links.
fn href_page(tag: &str) -> Option<String> {
    let h = tag.split("href=\"").nth(1)?.split('"').next()?;
    let p = h.strip_prefix("/wiki/").or_else(|| h.strip_prefix("./"))?;
    let page = url_decode(p.split('#').next()?).replace('_', " ");
    (!page.contains(':') && !page.is_empty()).then_some(page)
}

/// The anchor (`#Latin`) of the n-th link target in some HTML: (page, anchor) pairs of all dictionary links.
pub fn html_links(html: &str) -> Vec<(String, String)> {
    html.split("<a ").skip(1).filter_map(|a| {
        let tag = &a[..a.find('>').unwrap_or(a.len())];
        let h = tag.split("href=\"").nth(1)?.split('"').next()?;
        let p = h.strip_prefix("/wiki/").or_else(|| h.strip_prefix("./"))?;
        let mut it = p.splitn(2, '#');
        let page = url_decode(it.next()?).replace('_', " ");
        let anchor = url_decode(it.next().unwrap_or("")).replace('_', " ");
        (!page.contains(':') && !page.is_empty()).then_some((page, anchor))
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_templates() {
        let t = templates("* German: {{tt+|de|Hund|m}}, {{t|de|[[an]] [[den]] Fersen|tr=x}} {{q|formal}}");
        assert_eq!(t.len(), 3);
        assert_eq!(t[0].name, "tt+");
        assert_eq!(t[0].pos, vec!["de", "Hund", "m"]);
        assert_eq!(t[1].p(1), "[[an]] [[den]] Fersen");
        assert_eq!(t[1].get("tr"), Some("x"));
        // An unclosed table start is skipped, the closed one before it is found.
        let t = templates("{{trans-top|animal}}{{multitrans|data=");
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].p(0), "animal");
        let t = templates("{{K|spr=la|klassischlateinisch}} Hund");
        assert_eq!(t[0].pos, vec!["klassischlateinisch"]);
        assert_eq!(t[0].get("spr"), Some("la"));
    }

    #[test]
    fn wikitext_to_pieces() {
        let s = wiki_segs("[[Haustier]], dessen Vorfahre der [[Wolf]] ist<ref>Quelle {{x}}</ref> <!-- Kommentar -->");
        assert_eq!(plain(&s), "Haustier, dessen Vorfahre der Wolf ist");
        assert_eq!(s[0], Seg { t: "Haustier".into(), l: "Haustier".into() });
        let s = wiki_segs("[[gerissen]]er, ''[[w:Kundensprache|Kundensprache]]'' [[Datei:x.png|mini]]");
        assert_eq!(plain(&s), "gerissener, Kundensprache");
        assert_eq!(s[1].l, "");
        let e = expand("{{K|Bergbau}} ein [[Förderwagen]]", &|t| if t.name == "K" { format!("{}: ", t.pos.join(", ")) } else { String::new() });
        assert_eq!(plain(&wiki_segs(&e)), "Bergbau: ein Förderwagen");
    }

    #[test]
    fn html_to_pieces() {
        let h = r#"<span class="form-of-definition"><a rel="mw:WikiLink" href="/wiki/Appendix:Glossary#accusative_case" title="x">accusative</a> singular of <i lang="la"><a rel="mw:WikiLink" href="/wiki/canis#Latin" title="canis">canis</a></i></span> &amp; more"#;
        let s = html_segs(h);
        assert_eq!(plain(&s), "accusative singular of canis & more");
        assert!(s.iter().any(|x| x.l == "canis" && x.t == "canis"));
        assert!(s.iter().all(|x| !x.l.contains("Glossary")));
        assert_eq!(html_links(h), vec![("canis".to_string(), "Latin".to_string())]);
        let h = r#"to enjoy<style data-mw-deduplicate="x">.mw-parser-output .tag{font-style:italic}</style><span class="tag">[with infinitive]</span>"#;
        assert_eq!(plain(&html_segs(h)), "to enjoy[with infinitive]");
        assert_eq!(url_title("ἄνθρωπος x"), "%E1%BC%84%CE%BD%CE%B8%CF%81%CF%89%CF%80%CE%BF%CF%82_x");
        assert_eq!(url_decode("%CE%BB%CF%8C%CE%B3%CE%BF%CF%82"), "λόγος");
        assert_eq!(entities("a&nbsp;b &#955; &#x3bb; &bogus c"), "a b λ λ &bogus c");
    }
}
