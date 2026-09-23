//! Responses Proxy — converts OpenAI Responses API requests to Chat Completions
//! and proxies them to upstream Chat API providers.

pub mod app;
pub mod catalog;
pub mod config;
pub mod convert;
pub mod crypto;
pub mod handlers;
pub mod prompt;
pub mod rewrite;
pub mod store;
pub mod types;
pub mod upstream;
