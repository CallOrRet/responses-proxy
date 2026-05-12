use crate::types::item::InputItem;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::RwLock;

// ── Types ────────────────────────────────────────────────────────────────

/// A single node in the response chain. Each response stores its items
/// and a link to the previous response (for multi-turn conversation history).
#[derive(Clone)]
struct Entry {
    items: Vec<InputItem>,
    prev: Option<String>,
}

/// Thread-safe, disk-backed history store for response chains.
///
/// In-memory entries are persisted as JSONL snapshots in the history directory.
#[derive(Clone)]
pub struct History {
    inner: Arc<RwLock<HashMap<String, Entry>>>,
    dir: PathBuf,
}

/// Maximum number of history snapshots to keep per chain (older ones are deleted).
const MAX_SNAPSHOTS: usize = 5;

/// (response_id, items, previous_response_id)
type ChainNode = (String, Vec<InputItem>, Option<String>);

// ── Public API ───────────────────────────────────────────────────────────

impl History {
    pub fn new(dir: PathBuf) -> Self {
        std::fs::create_dir_all(&dir).ok();
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
            dir,
        }
    }

    /// Retrieve the full conversation chain for a response (memory → disk fallback).
    pub async fn get(&self, id: &str) -> Option<Vec<InputItem>> {
        {
            let g = self.inner.read().await;
            if g.contains_key(id) {
                return self.walk(&g, id);
            }
        }
        self.load_from_disk(id).await
    }

    /// Store a response entry and asynchronously persist it to disk.
    pub async fn set(&self, id: String, items: Vec<InputItem>, prev: Option<String>) {
        self.inner
            .write()
            .await
            .insert(id.clone(), Entry { items, prev });

        let chain = {
            let g = self.inner.read().await;
            collect_chain(&g, &id)
        };

        let dir = self.dir.clone();
        tokio::spawn(async move {
            if let Err(e) = write_snapshot(&dir, &id, &chain).await {
                tracing::warn!(id = %id, error = %e, "Failed to persist history snapshot");
            }
            // Prune old snapshots beyond MAX_SNAPSHOTS
            for old in chain.iter().map(|(c, _, _)| c).skip(MAX_SNAPSHOTS) {
                delete_file(&dir, old).await;
            }
        });
    }

    /// Remove an entire response chain from memory.
    pub async fn remove(&self, id: &str) {
        let g = self.inner.read().await;

        let mut ids = Vec::new();
        let mut cur = Some(id.to_string());
        while let Some(c) = cur {
            ids.push(c.clone());
            cur = g.get(&c).and_then(|e| e.prev.clone());
        }
        drop(g);

        let mut g = self.inner.write().await;
        for c in &ids {
            g.remove(c);
        }
    }

    /// Walk an in-memory chain, collecting all items from the root.
    fn walk(&self, g: &HashMap<String, Entry>, id: &str) -> Option<Vec<InputItem>> {
        let chain = collect_chain(g, id);
        if chain.is_empty() {
            return None;
        }
        Some(
            chain
                .iter()
                .rev()
                .flat_map(|(_, items, _)| items.clone())
                .collect(),
        )
    }

    /// Load a chain from a JSONL snapshot file; populates in-memory cache on success.
    async fn load_from_disk(&self, id: &str) -> Option<Vec<InputItem>> {
        let path = snapshot_path(&self.dir, id);
        let file = tokio::fs::File::open(&path).await.ok()?;

        let mut lines = BufReader::new(file).lines();
        let mut nodes: Vec<ChainNode> = Vec::new();
        let (mut cid, mut cprev, mut citems): (Option<String>, Option<String>, Vec<InputItem>) =
            (None, None, Vec::new());

        while let Ok(Some(line)) = lines.next_line().await {
            if line.is_empty() {
                continue;
            }

            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line)
                && v.get("_id").is_some()
            {
                // Header line: commit previous node and start new one
                if let Some(id) = cid.take() {
                    nodes.push((id, std::mem::take(&mut citems), cprev.take()));
                }
                cid = v["_id"].as_str().map(|s| s.to_string());
                cprev = v
                    .get("_prev")
                    .and_then(|s| s.as_str())
                    .map(|s| s.to_string());
            } else if cid.is_some()
                && let Ok(item) = serde_json::from_str::<InputItem>(&line)
            {
                citems.push(item);
            }
        }

        if let Some(id) = cid {
            nodes.push((id, citems, cprev));
        }

        if nodes.is_empty() {
            return None;
        }

        // Populate in-memory cache and collect all items
        let mut g = self.inner.write().await;
        let mut all = Vec::new();
        for (nid, items, prev) in &nodes {
            all.extend(items.clone());
            g.entry(nid.clone()).or_insert(Entry {
                items: items.clone(),
                prev: prev.clone(),
            });
        }

        Some(all)
    }
}

// ── Persistence helpers ──────────────────────────────────────────────────

fn snapshot_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{}.jsonl", id.strip_prefix("resp_").unwrap_or(id)))
}

/// Collect the full chain of entries from `id` back to the root.
fn collect_chain(g: &HashMap<String, Entry>, id: &str) -> Vec<ChainNode> {
    let mut ids = Vec::new();
    let mut cur = Some(id.to_string());
    while let Some(c) = cur {
        ids.push(c.clone());
        cur = g.get(&c).and_then(|e| e.prev.clone());
    }

    // Reverse to get chronologically oldest first
    ids.reverse();

    ids.iter()
        .filter_map(|cid| {
            g.get(cid)
                .map(|e| (cid.clone(), e.items.clone(), e.prev.clone()))
        })
        .collect()
}

/// Write a chain to a JSONL file: header lines with `_id`/`_prev` followed by items.
async fn write_snapshot(dir: &Path, id: &str, chain: &[ChainNode]) -> Result<(), std::io::Error> {
    let mut file = tokio::fs::File::create(snapshot_path(dir, id)).await?;

    for (cid, items, prev) in chain {
        let h = match prev {
            Some(p) => serde_json::json!({"_id": cid, "_prev": p}),
            None => serde_json::json!({"_id": cid}),
        };
        file.write_all(format!("{}\n", h).as_bytes()).await?;

        for item in items {
            file.write_all(format!("{}\n", serde_json::to_string(item).unwrap()).as_bytes())
                .await?;
        }
    }

    file.flush().await?;
    Ok(())
}

async fn delete_file(dir: &Path, id: &str) {
    let p = snapshot_path(dir, id);
    match tokio::fs::remove_file(&p).await {
        Ok(()) => tracing::info!(id = %id, path = %p.display(), "Deleted old history snapshot"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => tracing::warn!(id = %id, error = %e, "Failed to delete history snapshot"),
    }
}
