use crate::config::{ResolvedConfig, ResolvedProvider};
use crate::prompt;
use crate::store;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

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
    /// Memoized upstream-advertised context windows, keyed by `base_url::model`.
    /// A cached `None` records that the upstream had nothing to offer, so we
    /// don't re-fetch on every `/v1/models` request.
    context_window_cache: Arc<RwLock<HashMap<String, Option<i64>>>>,
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
            context_window_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Resolve the context window (tokens) to advertise to Codex for `provider`:
    /// the config override when set, otherwise the upstream-advertised value
    /// (fetched once and memoized), otherwise `None` so Codex uses its default.
    pub async fn resolve_context_window(&self, provider: &ResolvedProvider) -> Option<i64> {
        if let Some(cw) = provider.context_window {
            return Some(cw);
        }
        let key = format!("{}::{}", provider.base_url, provider.model);
        if let Some(cached) = self.context_window_cache.read().await.get(&key) {
            return *cached;
        }
        let fetched = crate::upstream::fetch_context_window(&self.http_client, provider).await;
        self.context_window_cache.write().await.insert(key, fetched);
        fetched
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
