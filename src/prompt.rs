use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

// ── Default prompts ──────────────────────────────────────────────────────

/// Default compact prompt, adapted from Claude Code's compaction behavior.
const DEFAULTS: &[(&str, &str)] = &[(
    "summary",
    r##"
CRITICAL: Respond with TEXT ONLY. Do NOT call any tools.
Summarize the conversation above, including all key decisions, code changes,
file edits, user requests, and their outcomes. Be detailed and specific.
"##,
)];

// ── Prompt store ─────────────────────────────────────────────────────────

/// Thread-safe prompt store that loads `.md` files from disk, with built-in defaults.
pub struct Prompt {
    prompts: Arc<RwLock<HashMap<String, String>>>,
}

impl Clone for Prompt {
    fn clone(&self) -> Self {
        Self {
            prompts: Arc::clone(&self.prompts),
        }
    }
}

impl Prompt {
    /// Load all `.md` files from `dir`, falling back to built-in defaults.
    pub fn load_from_dir(dir: PathBuf) -> Self {
        let mut prompts: HashMap<String, String> = HashMap::new();
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("md")
                    && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
                {
                    match std::fs::read_to_string(&path) {
                        Ok(c) => {
                            prompts.insert(stem.into(), c.trim().into());
                        }
                        Err(e) => {
                            tracing::warn!(key=%stem, error=%e, "Failed to read prompt file");
                        }
                    }
                }
            }
        }
        for &(key, def) in DEFAULTS {
            prompts.entry(key.into()).or_insert_with(|| def.into());
        }

        Self {
            prompts: Arc::new(RwLock::new(prompts)),
        }
    }

    pub fn set(&self, key: &str, value: String) {
        if let Ok(mut g) = self.prompts.write() {
            g.insert(key.into(), value.trim().into());
        }
    }

    /// Look up a prompt by key; falls back to defaults, then empty string.
    pub fn get(&self, key: &str) -> String {
        if let Ok(g) = self.prompts.read()
            && let Some(v) = g.get(key)
        {
            return v.clone();
        }
        DEFAULTS
            .iter()
            .find(|&&(k, _)| k == key)
            .map(|&(_, v)| v)
            .unwrap_or("")
            .into()
    }
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    fn d() -> PathBuf {
        std::env::temp_dir().join(format!("pt_{}", std::process::id()))
    }
    #[test]
    fn test_unknown() {
        assert_eq!(Prompt::load_from_dir(d()).get("x"), "");
    }
    #[test]
    fn test_set_get() {
        let p = Prompt::load_from_dir(d());
        p.set("k", "v".into());
        assert_eq!(p.get("k"), "v");
    }
    #[test]
    fn test_default() {
        assert!(
            Prompt::load_from_dir(d())
                .get("summary")
                .contains("CRITICAL")
        );
    }
    #[test]
    fn test_load() {
        let dir = std::env::temp_dir().join(format!("pt2_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("w.md"), "W").unwrap();
        let p = Prompt::load_from_dir(dir.clone());
        assert_eq!(p.get("w"), "W");
        std::fs::remove_dir_all(&dir).unwrap();
    }
    #[test]
    fn test_thread_safe() {
        let p = Arc::new(Prompt::load_from_dir(d()));
        let (a, b) = (p.clone(), p.clone());
        std::thread::spawn(move || a.set("a", "1".into()))
            .join()
            .unwrap();
        std::thread::spawn(move || b.set("b", "2".into()))
            .join()
            .unwrap();
        assert_eq!(p.get("a"), "1");
        assert_eq!(p.get("b"), "2");
    }
}
