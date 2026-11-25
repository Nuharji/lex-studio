//! Lookups already made, kept on disk (<app data>/lookups.json): the same word again comes at once, and without
//! internet the last answer is still there. At most MAX entries; the oldest go first.

use serde_json::{Map, Value};
use std::path::PathBuf;
#[cfg(test)]
use std::path::Path;

const MAX: usize = 3000;
/// After this many days a lookup is fetched again (when online).
pub const FRESH_DAYS: i64 = 30;

pub fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

pub fn key(word: &str, from: &str, to: &str) -> String {
    format!("{from}>{to}:{}", word.trim().to_lowercase())
}

pub struct Cache {
    path: Option<PathBuf>,
    map: Map<String, Value>,
}

impl Cache {
    pub fn open(path: Option<PathBuf>) -> Self {
        let map = path.as_deref().and_then(|p| std::fs::read(p).ok()).and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .and_then(|v| v.as_object().cloned()).unwrap_or_default();
        Cache { path, map }
    }

    /// The stored answer and its age in days.
    pub fn get(&self, key: &str) -> Option<(Value, i64)> {
        let e = self.map.get(key)?;
        let at = e.get("at").and_then(Value::as_i64).unwrap_or(0);
        Some((e.get("r")?.clone(), (now() - at) / 86_400_000))
    }

    pub fn put(&mut self, key: &str, r: Value) {
        self.map.insert(key.to_string(), serde_json::json!({"at": now(), "r": r}));
        if self.map.len() > MAX {
            let mut ages: Vec<(i64, String)> = self.map.iter().map(|(k, v)| (v.get("at").and_then(Value::as_i64).unwrap_or(0), k.clone())).collect();
            ages.sort();
            for (_, k) in ages.into_iter().take(self.map.len() - MAX) { self.map.remove(&k); }
        }
        self.save();
    }

    pub fn len(&self) -> usize { self.map.len() }

    pub fn clear(&mut self) { self.map.clear(); self.save(); }

    fn save(&self) {
        let Some(p) = &self.path else { return };
        if let Some(d) = p.parent() { let _ = std::fs::create_dir_all(d); }
        let tmp = p.with_extension("tmp");
        if std::fs::write(&tmp, serde_json::to_vec(&self.map).unwrap_or_default()).is_ok() { let _ = std::fs::rename(&tmp, p); }
    }

    #[cfg(test)]
    fn path(&self) -> Option<&Path> { self.path.as_deref() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_and_forgets() {
        let d = std::env::temp_dir().join(format!("lex-cache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let mut c = Cache::open(Some(d.join("lookups.json")));
        assert!(c.get("x").is_none());
        c.put(&key(" Canis ", "la", "de"), serde_json::json!({"word": "canis"}));
        let c2 = Cache::open(c.path().map(Path::to_path_buf));
        let (v, age) = c2.get("la>de:canis").unwrap();
        assert_eq!(v["word"], "canis");
        assert_eq!(age, 0);
        for i in 0..MAX + 5 { c.map.insert(format!("k{i}"), serde_json::json!({"at": i, "r": 1})); }
        c.put("new", serde_json::json!(1));
        assert_eq!(c.len(), MAX);
        assert!(c.get("new").is_some() && c.get("k0").is_none());
        c.clear();
        assert_eq!(Cache::open(Some(d.join("lookups.json"))).len(), 0);
        let _ = std::fs::remove_dir_all(d);
    }
}
