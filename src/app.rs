use crate::config::ResolvedConfig;
use crate::prompt;
use crate::store;
use std::path::PathBuf;

// ── Filesystem helpers ───────────────────────────────────────────────────

/// Returns the application home directory: `~/.responses-proxy`
pub fn home_dir() -> PathBuf {
    match dirs::home_dir() {
        Some(h) => h.join(".responses-proxy"),
        None => PathBuf::from("."),
    }
}

/// Creates the home directory and its `prompts` / `store` subdirectories.
pub fn ensure_dirs() {
    let home = home_dir();
    std::fs::create_dir_all(&home).ok();
    std::fs::create_dir_all(home.join("prompts")).ok();
    std::fs::create_dir_all(home.join("messages")).ok();
}

// ── Shared application state ─────────────────────────────────────────────

/// Central state passed to every request handler via Axum extractors.
#[derive(Clone)]
pub struct State {
    http_client: reqwest::Client,
    config: ResolvedConfig,
    /// Optional AES-256 key for compact content encryption (32 bytes from hex).
    compact_key: Option<[u8; 32]>,
    prompts: prompt::Prompt,
    store: store::Store,
}

impl State {
    pub fn new(config: ResolvedConfig) -> Self {
        let home = home_dir();

        let compact_key = if config.compact_encryption_key.is_empty() {
            None
        } else {
            match hex::decode(&config.compact_encryption_key) {
                Ok(b) if b.len() == 32 => {
                    let mut k = [0u8; 32];
                    k.copy_from_slice(&b);
                    Some(k)
                }
                _ => {
                    tracing::warn!("compact_encryption_key must be 64 hex chars.");
                    None
                }
            }
        };

        let prompts = prompt::Prompt::load_from_dir(home.join("prompts"));
        let store = store::Store::with_dir(home.clone());

        Self {
            http_client: reqwest::Client::new(),
            config,
            compact_key,
            prompts,
            store,
        }
    }

    pub fn config(&self) -> &ResolvedConfig {
        &self.config
    }

    pub fn compact_key(&self) -> Option<&[u8; 32]> {
        self.compact_key.as_ref()
    }

    pub fn prompts(&self) -> &prompt::Prompt {
        &self.prompts
    }

    pub fn http_client(&self) -> &reqwest::Client {
        &self.http_client
    }

    pub fn store(&self) -> &store::Store {
        &self.store
    }
}
