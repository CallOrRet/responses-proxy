use std::path::{Path, PathBuf};
use std::time::Duration;

use responses_proxy::store::Store;
use responses_proxy::types::chat::{AssistantMessage, MessageRequest, UserMessage};

fn temp_store_dir() -> PathBuf {
    std::env::temp_dir().join(format!(
        "responses_proxy_store_files_{}_{}",
        std::process::id(),
        uuid::Uuid::new_v4().to_string().replace('-', "")
    ))
}

fn user_msg(text: &str) -> MessageRequest {
    MessageRequest::User(UserMessage {
        content: responses_proxy::types::chat::UserContent::Text(text.to_string()),
        name: None,
    })
}

fn assistant_msg(text: &str) -> MessageRequest {
    MessageRequest::Assistant(AssistantMessage {
        content: Some(responses_proxy::types::chat::AssistantContent::Text(
            text.to_string(),
        )),
        name: None,
        refusal: None,
        audio: None,
        reasoning_content: None,
        tool_calls: None,
        function_call: None,
    })
}

fn message_text(msg: &MessageRequest) -> &str {
    match msg {
        MessageRequest::User(m) => match &m.content {
            responses_proxy::types::chat::UserContent::Text(t) => t,
            _ => panic!("expected text content"),
        },
        MessageRequest::Assistant(m) => match m.content.as_ref().unwrap() {
            responses_proxy::types::chat::AssistantContent::Text(t) => t,
            _ => panic!("expected text content"),
        },
        _ => panic!("expected user or assistant message"),
    }
}

async fn wait_for_path(path: &Path) {
    for _ in 0..50 {
        if path.exists() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("path was not written: {}", path.display());
}

#[tokio::test]
async fn store_writes_messages_jsonl_and_recovers_from_disk() {
    let dir = temp_store_dir();
    let store = Store::with_dir(dir.clone());

    let first_msgs = vec![user_msg("first turn"), assistant_msg("first reply")];
    let second_msgs = vec![user_msg("second turn"), assistant_msg("second reply")];

    store.put("resp_store_first".into(), first_msgs).await;
    store.put("resp_store_second".into(), second_msgs).await;

    let first_messages_path = dir.join("messages").join("store_first.jsonl");
    let second_messages_path = dir.join("messages").join("store_second.jsonl");

    wait_for_path(&first_messages_path).await;
    wait_for_path(&second_messages_path).await;

    // Verify messages JSONL on disk
    let messages_jsonl = tokio::fs::read_to_string(&second_messages_path)
        .await
        .unwrap();
    let decoded_messages: Vec<MessageRequest> = messages_jsonl
        .lines()
        .map(|line| serde_json::from_str::<MessageRequest>(line).unwrap())
        .collect();
    assert_eq!(decoded_messages.len(), 2);
    assert_eq!(message_text(&decoded_messages[0]), "second turn");
    assert_eq!(message_text(&decoded_messages[1]), "second reply");

    // Recover from disk with a new Store instance
    let recovered = Store::with_dir(dir.clone());
    let recovered_messages = recovered.get("resp_store_first").await.unwrap();
    assert_eq!(recovered_messages.len(), 2);
    assert_eq!(message_text(&recovered_messages[0]), "first turn");
    assert_eq!(message_text(&recovered_messages[1]), "first reply");

    let recovered_second = recovered.get("resp_store_second").await.unwrap();
    assert_eq!(recovered_second.len(), 2);
    assert_eq!(message_text(&recovered_second[0]), "second turn");

    assert!(recovered.get("resp_store_first").await.is_some());

    let _ = tokio::fs::remove_dir_all(dir).await;
}

#[tokio::test]
async fn namespaced_ids_isolate_history_and_disk_layout() {
    use responses_proxy::store::namespaced_id;

    let dir = temp_store_dir();
    let store = Store::with_dir(dir.clone());

    // Same raw response id minted under two namespaces → distinct keys, so two
    // parallel Codex instances can't read each other's history.
    let raw = "resp_deadbeef";
    let a = namespaced_id("alpha", raw);
    let b = namespaced_id("beta", raw);
    assert_ne!(a, b);

    store.put(a.clone(), vec![user_msg("alpha history")]).await;
    store.put(b.clone(), vec![user_msg("beta history")]).await;

    // Each namespace persists under its own subdirectory.
    let a_path = dir.join("messages").join("alpha").join("deadbeef.jsonl");
    let b_path = dir.join("messages").join("beta").join("deadbeef.jsonl");
    wait_for_path(&a_path).await;
    wait_for_path(&b_path).await;

    // Recover from a fresh Store (disk-only) and confirm no cross-talk.
    let recovered = Store::with_dir(dir.clone());
    assert_eq!(
        message_text(&recovered.get(&a).await.unwrap()[0]),
        "alpha history"
    );
    assert_eq!(
        message_text(&recovered.get(&b).await.unwrap()[0]),
        "beta history"
    );
    // The un-namespaced raw id belongs to neither namespace.
    assert!(recovered.get(raw).await.is_none());

    let _ = tokio::fs::remove_dir_all(dir).await;
}

#[tokio::test]
async fn missing_entry_is_a_miss_not_empty_history() {
    // A genuine cache miss must be None, not Some(vec![]) — otherwise a lost
    // history would masquerade as an empty (but valid) conversation.
    let dir = temp_store_dir();
    let store = Store::with_dir(dir.clone());
    assert!(store.get("resp_never_written").await.is_none());
    let _ = tokio::fs::remove_dir_all(dir).await;
}
