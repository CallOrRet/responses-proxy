/// Integration tests — these hit the real DeepSeek API through the proxy server.
/// The proxy server must be running separately:
///   DOWNSTREAM_API_KEY=sk-... cargo run
///
/// Then run tests:
///   cargo test --test integration_test -- --nocapture
use serde_json::{Value, json};

fn proxy_url() -> String {
    std::env::var("PROXY_URL").unwrap_or_else(|_| "http://localhost:3000".to_string())
}

async fn send_responses_request(body: Value, expect_status: u16) -> Value {
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/v1/responses", proxy_url()))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .expect("Failed to send request to proxy");

    let status = response.status().as_u16();
    let response_body: Value = response.json().await.expect("Failed to parse response");

    if status != expect_status {
        println!(
            "Expected status {expect_status}, got {status}. Body: {}",
            serde_json::to_string_pretty(&response_body).unwrap()
        );
    }

    response_body
}

#[tokio::test]
async fn test_basic_completion() {
    let req = json!({
        "model": "deepseek-v4-pro",
        "input": "Reply with exactly: Hello, World!"
    });

    let resp = send_responses_request(req, 200).await;

    println!("Response: {}", serde_json::to_string_pretty(&resp).unwrap());

    assert_eq!(resp["object"], "response");
    assert_eq!(resp["status"], "completed");
    assert!(resp["id"].as_str().unwrap().starts_with("resp_"));

    let output = resp["output"].as_array().expect("output should be array");
    assert!(!output.is_empty());
    assert_eq!(output[0]["type"], "message");
    assert_eq!(output[0]["role"], "assistant");

    let content = output[0]["content"]
        .as_array()
        .expect("content should be array");
    assert_eq!(content[0]["type"], "output_text");
    assert!(
        content[0]["text"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("hello")
    );

    let usage = &resp["usage"];
    assert!(usage["input_tokens"].as_u64().unwrap() > 0);
    assert!(usage["total_tokens"].as_u64().unwrap() > 0);
}

#[tokio::test]
async fn test_string_input() {
    let req = json!({
        "model": "deepseek-v4-pro",
        "input": "What is 2+2? Reply with just the number."
    });

    let resp = send_responses_request(req, 200).await;
    let text = resp["output"][0]["content"][0]["text"].as_str().unwrap();
    println!("Answer: {text}");
    assert!(text.contains("4"));
}

#[tokio::test]
async fn test_with_instructions() {
    let req = json!({
        "model": "deepseek-v4-pro",
        "instructions": "You are a translator. Always translate user messages to Chinese.",
        "input": "Good morning"
    });

    let resp = send_responses_request(req, 200).await;
    let text = resp["output"][0]["content"][0]["text"].as_str().unwrap();
    println!("Translation: {text}");
    assert!(
        text.chars().any(|c| c as u32 > 127),
        "Expected Chinese characters in output"
    );
}

#[tokio::test]
async fn test_array_input_with_messages() {
    let req = json!({
        "model": "deepseek-v4-pro",
        "input": [
            {
                "type": "message",
                "role": "system",
                "content": [{"type": "input_text", "text": "Respond in JSON format always."}]
            },
            {
                "type": "message",
                "role": "user",
                "content": [{"type": "input_text", "text": "Say hello"}]
            }
        ]
    });

    let resp = send_responses_request(req, 200).await;
    assert_eq!(resp["status"], "completed");
    let text = resp["output"][0]["content"][0]["text"].as_str().unwrap();
    println!("Response: {text}");
    assert!(!text.is_empty());
}

#[tokio::test]
async fn test_temperature_and_max_tokens() {
    let req = json!({
        "model": "deepseek-v4-pro",
        "input": "Write numbers 1 to 5.",
        "temperature": 0.0,
        "max_output_tokens": 30
    });

    let resp = send_responses_request(req, 200).await;
    assert_eq!(resp["status"], "completed");
    let usage = &resp["usage"];
    // With low max_tokens, output should be short
    assert!(
        usage["output_tokens"].as_u64().unwrap() <= 40,
        "Output tokens should be limited"
    );
}

#[tokio::test]
async fn test_multi_turn_conversation() {
    let req = json!({
        "model": "deepseek-v4-pro",
        "input": [
            {
                "type": "message",
                "role": "user",
                "content": [{"type": "input_text", "text": "My name is Alice."}]
            },
            {
                "type": "message",
                "role": "assistant",
                "content": [{"type": "input_text", "text": "Hello Alice, nice to meet you!"}]
            },
            {
                "type": "message",
                "role": "user",
                "content": [{"type": "input_text", "text": "What is my name?"}]
            }
        ]
    });

    let resp = send_responses_request(req, 200).await;
    let text = resp["output"][0]["content"][0]["text"].as_str().unwrap();
    println!("Response: {text}");
    assert!(
        text.to_lowercase().contains("alice"),
        "Model should remember the name Alice from conversation history"
    );
}

#[tokio::test]
async fn test_tool_calling() {
    let req = json!({
        "model": "deepseek-v4-pro",
        "input": "What's the weather like in Tokyo? Use the get_weather function.",
        "tools": [{
            "type": "function",
            "name": "get_weather",
            "description": "Get the current weather for a city",
            "parameters": {
                "type": "object",
                "properties": {
                    "city": {
                        "type": "string",
                        "description": "The city name"
                    }
                },
                "required": ["city"]
            }
        }],
        "tool_choice": "auto"
    });

    let resp = send_responses_request(req, 200).await;
    println!(
        "Tool test response: {}",
        serde_json::to_string_pretty(&resp).unwrap()
    );

    assert_eq!(resp["status"], "completed");

    let output = resp["output"].as_array().unwrap();
    let has_function_call = output.iter().any(|item| item["type"] == "function_call");

    if has_function_call {
        let fc = output
            .iter()
            .find(|item| item["type"] == "function_call")
            .unwrap();
        println!(
            "Function call: {} with args: {}",
            fc["name"], fc["arguments"]
        );
        assert_eq!(fc["name"], "get_weather");
    } else {
        println!("Model didn't call function - checking text response");
        let text = output[0]["content"][0]["text"].as_str().unwrap();
        assert!(!text.is_empty());
    }
}

#[tokio::test]
async fn test_model_field_preserved() {
    let req = json!({
        "model": "deepseek-v4-pro",
        "input": "Say 'test'"
    });

    let resp = send_responses_request(req, 200).await;
    // The model field should be preserved from the original request
    assert_eq!(resp["model"], "deepseek-v4-pro");
}

#[tokio::test]
async fn test_invalid_request() {
    // Missing required "model" field
    let req = json!({
        "input": "Hello"
    });

    let resp = send_responses_request(req, 400).await;
    assert!(resp.get("error").is_some());
}

#[tokio::test]
async fn test_response_structure_consistency() {
    // Run multiple requests and verify structure consistency
    for _ in 0..3 {
        let req = json!({
            "model": "deepseek-v4-pro",
            "input": "Say exactly: OK"
        });

        let resp = send_responses_request(req, 200).await;

        // Verify all mandatory fields are present
        assert!(
            resp.get("id").and_then(|v| v.as_str()).is_some(),
            "Missing id"
        );
        assert_eq!(resp["object"], "response", "Missing/invalid object");
        assert!(resp.get("created_at").is_some(), "Missing created_at");
        assert!(resp.get("status").is_some(), "Missing status");
        assert!(resp.get("model").is_some(), "Missing model");
        assert!(
            resp.get("output").and_then(|v| v.as_array()).is_some(),
            "Missing output array"
        );
        assert!(resp.get("usage").is_some(), "Missing usage");

        // Verify output item structure
        let output = resp["output"].as_array().unwrap();
        if !output.is_empty() {
            let msg = &output[0];
            assert!(msg.get("id").is_some(), "Output message missing id");
            assert_eq!(msg["type"], "message");
            assert_eq!(msg["role"], "assistant");
            assert!(msg.get("status").is_some());
            assert!(msg.get("content").and_then(|c| c.as_array()).is_some());
        }
    }
}
