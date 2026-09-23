//! In-memory message store with TTL expiry + disk persistence.
//!
//! Each entry is persisted as a JSONL file under `messages/{id}.jsonl`.

use crate::types::chat::{MessageRequest, ToolRequest};
use rustc_hash::FxHashMap as HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::RwLock;

const DEFAULT_TTL: Duration = Duration::from_secs(30 * 60);

// ── StoredMessages ───────────────────────────────────────────────────────────

#[derive(Clone)]
struct StoredMessages {
    messages: Vec<MessageRequest>,
    created_at: Instant,
}

#[derive(Clone)]
struct StoredTools {
    tools: Vec<ToolRequest>,
    /// Names Codex declared as freeform `custom` tools (code-mode `exec` etc.).
    /// Needed to re-emit the model's `function_call` as the `custom_tool_call`
    /// shape Codex expects; without it Codex cancels the call it didn't declare.
    custom_names: std::collections::HashSet<String>,
    /// Maps tool name → namespace name for tools declared inside a Codex
    /// `namespace` bundle (e.g. `spawn_agent` → `"collaboration"`). Needed to
    /// restore the `namespace` tag Chat Completions can't carry on the wire;
    /// without it Codex's `(name, namespace)`-keyed registry rejects the call
    /// as `"unsupported call: <name>"`. Empty whenever no namespace tools were
    /// declared, e.g. when Codex's own multi-agent feature flags are off.
    tool_namespaces: std::collections::HashMap<String, String>,
    created_at: Instant,
}

// ── Store ────────────────────────────────────────────────────────────────────

/// Thread-safe, in-memory message store with TTL cleanup and disk persistence.
#[derive(Clone)]
pub struct Store {
    inner: Arc<RwLock<HashMap<String, StoredMessages>>>,
    /// gpt-5.6 code-mode tool registry keyed by response ID. Codex delivers its
    /// `additional_tools` only on a new user turn; tool-result continuations
    /// reference `previous_response_id` and omit them, so the registry is cached
    /// here and restored on those continuations. In-memory only (TTL-bounded) —
    /// a new user turn always re-supplies the tools.
    tools: Arc<RwLock<HashMap<String, StoredTools>>>,
    ttl: Duration,
    dir: Option<PathBuf>,
    cancel_tokens: Arc<RwLock<HashMap<String, tokio::sync::watch::Sender<bool>>>>,
}

impl Store {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::default())),
            tools: Arc::new(RwLock::new(HashMap::default())),
            ttl: DEFAULT_TTL,
            dir: None,
            cancel_tokens: Arc::new(RwLock::new(HashMap::default())),
        }
    }

    pub fn with_dir(dir: PathBuf) -> Self {
        std::fs::create_dir_all(dir.join("messages")).ok();
        Self {
            inner: Arc::new(RwLock::new(HashMap::default())),
            tools: Arc::new(RwLock::new(HashMap::default())),
            ttl: DEFAULT_TTL,
            dir: Some(dir),
            cancel_tokens: Arc::new(RwLock::new(HashMap::default())),
        }
    }

    // ── CRUD ──────────────────────────────────────────────────────────────

    /// Store messages for a response ID.
    pub async fn put(&self, id: String, messages: Vec<MessageRequest>) {
        self.inner.write().await.insert(
            id.clone(),
            StoredMessages {
                messages,
                created_at: Instant::now(),
            },
        );

        if let Some(ref dir) = self.dir {
            let dir = dir.clone();
            let id_c = id.clone();
            let msgs = self
                .inner
                .read()
                .await
                .get(&id_c)
                .map(|e| e.messages.clone());
            tokio::spawn(async move {
                if let Some(msgs) = msgs {
                    let path = messages_path(&dir, &id_c);
                    if let Some(parent) = path.parent() {
                        tokio::fs::create_dir_all(parent).await.ok();
                    }
                    if let Err(e) = write_messages(&path, &msgs).await {
                        tracing::error!(id = %id_c, error = %e, "Failed to persist messages");
                    }
                }
            });
        }
    }

    /// Cache the gpt-5.6 code-mode tool registry, its custom-tool name set, and
    /// its tool→namespace map for a response ID. No-op when all three are empty
    /// so non-code-mode turns don't allocate entries.
    pub async fn put_tools(
        &self,
        id: String,
        tools: Vec<ToolRequest>,
        custom_names: std::collections::HashSet<String>,
        tool_namespaces: std::collections::HashMap<String, String>,
    ) {
        if tools.is_empty() && custom_names.is_empty() && tool_namespaces.is_empty() {
            return;
        }
        self.tools.write().await.insert(
            id,
            StoredTools {
                tools,
                custom_names,
                tool_namespaces,
                created_at: Instant::now(),
            },
        );
    }

    /// Retrieve the cached tool registry by ID. Returns None if not found or
    /// expired.
    pub async fn get_tools(&self, id: &str) -> Option<Vec<ToolRequest>> {
        let g = self.tools.read().await;
        let entry = g.get(id)?;
        if entry.created_at.elapsed() <= self.ttl {
            return Some(entry.tools.clone());
        }
        drop(g);
        self.tools.write().await.remove(id);
        None
    }

    /// Retrieve the cached custom-tool name set by ID. Returns None if not found
    /// or expired.
    pub async fn get_custom_names(&self, id: &str) -> Option<std::collections::HashSet<String>> {
        let g = self.tools.read().await;
        let entry = g.get(id)?;
        if entry.created_at.elapsed() <= self.ttl {
            return Some(entry.custom_names.clone());
        }
        drop(g);
        self.tools.write().await.remove(id);
        None
    }

    /// Retrieve the cached tool→namespace map by ID. Returns None if not found
    /// or expired.
    pub async fn get_tool_namespaces(
        &self,
        id: &str,
    ) -> Option<std::collections::HashMap<String, String>> {
        let g = self.tools.read().await;
        let entry = g.get(id)?;
        if entry.created_at.elapsed() <= self.ttl {
            return Some(entry.tool_namespaces.clone());
        }
        drop(g);
        self.tools.write().await.remove(id);
        None
    }

    /// Retrieve stored messages by ID. Returns None if not found or expired.
    pub async fn get(&self, id: &str) -> Option<Vec<MessageRequest>> {
        {
            let g = self.inner.read().await;
            if let Some(entry) = g.get(id) {
                if entry.created_at.elapsed() <= self.ttl {
                    return Some(entry.messages.clone());
                }
                drop(g);
                self.inner.write().await.remove(id);
                self.delete_disk_files(id);
                return None;
            }
        }
        self.load_messages_from_disk(id).await
    }

    /// Delete a single entry.
    pub async fn delete(&self, id: &str) -> bool {
        let existed = self.inner.write().await.remove(id).is_some();
        self.delete_disk_files(id);
        existed
    }

    /// Remove all expired entries.
    pub async fn sweep_expired(&self) {
        let expired: Vec<String> = {
            let g = self.inner.read().await;
            g.iter()
                .filter(|(_, v)| v.created_at.elapsed() > self.ttl)
                .map(|(k, _)| k.clone())
                .collect()
        };
        for k in &expired {
            self.inner.write().await.remove(k);
            self.delete_disk_files(k);
        }
        {
            let expired_tools: Vec<String> = {
                let g = self.tools.read().await;
                g.iter()
                    .filter(|(_, v)| v.created_at.elapsed() > self.ttl)
                    .map(|(k, _)| k.clone())
                    .collect()
            };
            if !expired_tools.is_empty() {
                let mut g = self.tools.write().await;
                for k in &expired_tools {
                    g.remove(k);
                }
            }
        }
        // Disk sweep. The memory maps above never covered on-disk files, so a
        // message JSONL written once and never re-read used to live forever
        // (unbounded growth, and stale context that could resurface on a reused
        // id). Delete stale files directly under messages/ and each namespace
        // subdirectory.
        if let Some(ref dir) = self.dir {
            let root = dir.join("messages");
            sweep_stale_files(&root, self.ttl).await;
            if let Ok(mut rd) = tokio::fs::read_dir(&root).await {
                while let Ok(Some(entry)) = rd.next_entry().await {
                    if entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false) {
                        sweep_stale_files(&entry.path(), self.ttl).await;
                    }
                }
            }
        }
        if !expired.is_empty() {
            tracing::info!(count = expired.len(), "Swept expired entries from store");
        }
    }

    pub fn start_sweep_task(&self) {
        let store = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(300));
            loop {
                interval.tick().await;
                store.sweep_expired().await;
            }
        });
    }

    // ── Cancellation ──────────────────────────────────────────────────────

    pub async fn register_cancel_token(&self, id: &str) -> tokio::sync::watch::Receiver<bool> {
        let (tx, rx) = tokio::sync::watch::channel(false);
        self.cancel_tokens.write().await.insert(id.to_string(), tx);
        rx
    }

    pub async fn cancel_in_flight(&self, id: &str) -> bool {
        if let Some(tx) = self.cancel_tokens.write().await.remove(id) {
            let _ = tx.send(true);
            true
        } else {
            false
        }
    }

    pub async fn unregister_cancel_token(&self, id: &str) {
        self.cancel_tokens.write().await.remove(id);
    }

    // ── Disk persistence ──────────────────────────────────────────────────

    fn delete_disk_files(&self, id: &str) {
        if let Some(ref dir) = self.dir {
            delete_messages_file(dir, id);
        }
    }

    async fn load_messages_from_disk(&self, id: &str) -> Option<Vec<MessageRequest>> {
        let dir = self.dir.as_ref()?;
        let path = messages_path(dir, id);
        // Respect the TTL against the file's mtime so stale on-disk history is
        // not resurrected with a fresh lifetime — previously every read reset
        // `created_at` to now, so disk entries never expired.
        let age = tokio::fs::metadata(&path)
            .await
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|mt| mt.elapsed().ok())
            .unwrap_or_default();
        if age > self.ttl {
            self.delete_disk_files(id);
            return None;
        }
        let msgs = read_messages(&path).await?;
        let created_at = Instant::now().checked_sub(age).unwrap_or_else(Instant::now);
        self.inner.write().await.insert(
            id.to_string(),
            StoredMessages {
                messages: msgs.clone(),
                created_at,
            },
        );
        Some(msgs)
    }
}

impl Default for Store {
    fn default() -> Self {
        Self::new()
    }
}

// ── Persistence helpers ──────────────────────────────────────────────────────

/// Restrict a namespace / id component to a filesystem- and key-safe charset.
/// Also the guard against path traversal from a client-supplied
/// `previous_response_id` reaching the disk path.
fn sanitize_component(s: &str) -> String {
    let out: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if out.is_empty() { "_".to_string() } else { out }
}

/// Embed a namespace into a freshly minted `resp_…` id so the store isolates it
/// (distinct memory key + on-disk subdirectory). Codex round-trips the id as
/// `previous_response_id`, so later reads resolve the namespaced entry with no
/// extra plumbing. An empty namespace returns the id unchanged (backward
/// compatible with existing entries and on-disk files).
pub fn namespaced_id(ns: &str, id: &str) -> String {
    if ns.is_empty() {
        return id.to_string();
    }
    let ns = sanitize_component(ns);
    match id.strip_prefix("resp_") {
        Some(rest) => format!("resp_{ns}.{rest}"),
        None => format!("{ns}.{id}"),
    }
}

/// Split a (possibly namespaced) response id into its on-disk `(subdir, stem)`.
/// `resp_<uuid>` → `(None, "<uuid>")`; `resp_<ns>.<uuid>` → `(Some("<ns>"),
/// "<uuid>")`. Both parts are sanitized. The `resp_` prefix carries no routing
/// information and is stripped.
fn disk_parts(id: &str) -> (Option<String>, String) {
    let rest = id.strip_prefix("resp_").unwrap_or(id);
    match rest.split_once('.') {
        Some((ns, stem)) if !ns.is_empty() && !stem.is_empty() => {
            (Some(sanitize_component(ns)), sanitize_component(stem))
        }
        _ => (None, sanitize_component(rest)),
    }
}

fn messages_path(dir: &Path, id: &str) -> PathBuf {
    let (ns, stem) = disk_parts(id);
    let base = dir.join("messages");
    let base = match ns {
        Some(ns) => base.join(ns),
        None => base,
    };
    base.join(format!("{stem}.jsonl"))
}

fn delete_messages_file(dir: &Path, id: &str) {
    let path = messages_path(dir, id);
    tokio::spawn(async move {
        let _ = tokio::fs::remove_file(&path).await;
    });
}

/// Delete message files directly under `dir` whose mtime is older than `ttl`.
/// Not recursive — the sweeper calls it once per directory (messages root and
/// each namespace subdirectory).
async fn sweep_stale_files(dir: &Path, ttl: Duration) {
    let Ok(mut rd) = tokio::fs::read_dir(dir).await else {
        return;
    };
    while let Ok(Some(entry)) = rd.next_entry().await {
        if !entry
            .file_type()
            .await
            .map(|t| t.is_file())
            .unwrap_or(false)
        {
            continue;
        }
        let p = entry.path();
        let stale = tokio::fs::metadata(&p)
            .await
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|mt| mt.elapsed().ok())
            .map(|age| age > ttl)
            .unwrap_or(false);
        if stale {
            let _ = tokio::fs::remove_file(&p).await;
        }
    }
}

fn serde_to_io_err(e: serde_json::Error) -> std::io::Error {
    std::io::Error::other(e)
}

async fn write_messages(path: &Path, items: &[MessageRequest]) -> Result<(), std::io::Error> {
    let mut file = tokio::fs::File::create(path).await?;
    for item in items {
        let json = serde_json::to_string(item).map_err(serde_to_io_err)?;
        file.write_all(format!("{}\n", json).as_bytes()).await?;
    }
    file.flush().await?;
    tracing::debug!("saved {} messages to {:?}", items.len(), path);
    Ok(())
}

async fn read_messages(path: &Path) -> Option<Vec<MessageRequest>> {
    if !path.exists() {
        return None;
    }
    let file = tokio::fs::File::open(path).await.ok()?;
    let mut lines = BufReader::new(file).lines();
    let mut messages = Vec::new();
    while let Ok(Some(line)) = lines.next_line().await {
        if line.is_empty() {
            continue;
        }
        if let Ok(message) = serde_json::from_str::<MessageRequest>(&line) {
            messages.push(message);
        }
    }
    Some(messages)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespaced_id_empty_ns_is_unchanged() {
        // Backward compatible: no namespace → the id (and thus its disk path) is
        // untouched, so existing entries keep resolving.
        assert_eq!(namespaced_id("", "resp_abc123"), "resp_abc123");
    }

    #[test]
    fn namespaced_id_embeds_ns_after_prefix() {
        assert_eq!(namespaced_id("alpha", "resp_abc123"), "resp_alpha.abc123");
        // A non-resp id still gets namespaced (defensive; mint sites always pass
        // resp_ ids).
        assert_eq!(namespaced_id("alpha", "abc123"), "alpha.abc123");
    }

    #[test]
    fn namespaced_id_sanitizes_ns() {
        // Path-traversal / key-injection characters are neutralized.
        assert_eq!(
            namespaced_id("a/../b", "resp_x"),
            "resp_a____b.x",
            "slashes and dots collapse to underscores"
        );
    }

    #[test]
    fn disk_parts_plain_id_has_no_subdir() {
        let (ns, stem) = disk_parts("resp_deadbeef");
        assert_eq!(ns, None);
        assert_eq!(stem, "deadbeef");
    }

    #[test]
    fn disk_parts_namespaced_id_splits_subdir_and_stem() {
        let (ns, stem) = disk_parts("resp_alpha.deadbeef");
        assert_eq!(ns.as_deref(), Some("alpha"));
        assert_eq!(stem, "deadbeef");
    }

    #[test]
    fn messages_path_routes_namespace_to_subdir() {
        let root = Path::new("/tmp/store");
        assert_eq!(
            messages_path(root, "resp_deadbeef"),
            root.join("messages").join("deadbeef.jsonl")
        );
        assert_eq!(
            messages_path(root, "resp_alpha.deadbeef"),
            root.join("messages").join("alpha").join("deadbeef.jsonl")
        );
    }

    #[test]
    fn namespaced_id_and_disk_parts_round_trip() {
        // The stem survives a mint→resolve round-trip so the persisted file and
        // the later lookup agree on a path.
        let minted = namespaced_id("beta", "resp_cafef00d");
        let (ns, stem) = disk_parts(&minted);
        assert_eq!(ns.as_deref(), Some("beta"));
        assert_eq!(stem, "cafef00d");
    }
}
