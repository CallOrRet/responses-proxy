//! Responses Proxy — converts OpenAI Responses API requests to Chat Completions
//! and proxies them to upstream providers (DeepSeek, OpenAI, etc.).

pub mod app;
pub mod config;
pub mod convert;
pub mod crypto;
pub mod handlers;
pub mod history;
pub mod prompt;
pub mod types;
pub mod validation;
