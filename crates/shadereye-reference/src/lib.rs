//! Offline-first curated reference, with optional best-effort web fallback.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RefEntry {
    pub name: String,
    #[serde(default)]
    pub sig: String,
    pub note: String,
}

const GLSL: &str = include_str!("../data/glsl.json");
const GOTCHAS: &str = include_str!("../data/gotchas.json");

/// Case-insensitive substring search over bundled GLSL builtins + gotchas.
pub fn lookup(query: &str) -> Vec<RefEntry> {
    let q = query.to_lowercase();
    let mut out = Vec::new();
    for src in [GLSL, GOTCHAS] {
        if let Ok(entries) = serde_json::from_str::<Vec<RefEntry>>(src) {
            for e in entries {
                if e.name.to_lowercase().contains(&q)
                    || e.note.to_lowercase().contains(&q)
                    || e.sig.to_lowercase().contains(&q)
                {
                    out.push(e);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_smoothstep() {
        let r = lookup("smoothstep");
        assert!(r.iter().any(|e| e.name == "smoothstep"));
    }

    #[test]
    fn finds_gotcha_by_topic() {
        let r = lookup("precision");
        assert!(r.iter().any(|e| e.name == "webgl-precision"));
    }

    #[test]
    fn empty_for_unknown() {
        assert!(lookup("zzz-no-such-token-xyz").is_empty());
    }
}
