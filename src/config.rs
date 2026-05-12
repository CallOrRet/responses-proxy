use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::time::Duration;

// ── Raw configuration (from YAML) ────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub models: HashMap<String, ModelEntry>,
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_listen")]
    pub listen: String,
    #[serde(default = "default_timeout")]
    pub timeout: u64,
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub cors: CorsConfig,
    #[serde(default = "default_tool_allowlist")]
    pub tool_type_allowlist: Vec<String>,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default)]
    pub compact_encryption_key: String,
}

fn default_log_level() -> String {
    "info".into()
}

fn default_tool_allowlist() -> Vec<String> {
    vec!["function".into()]
}

fn default_listen() -> String {
    "0.0.0.0:3000".into()
}

fn default_timeout() -> u64 {
    600
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen: default_listen(),
            timeout: default_timeout(),
            auth: AuthConfig::default(),
            cors: CorsConfig::default(),
            tool_type_allowlist: default_tool_allowlist(),
            log_level: default_log_level(),
            compact_encryption_key: String::new(),
        }
    }
}

// ── Sub-config sections ──────────────────────────────────────────────────

#[derive(Debug, Default, Deserialize)]
pub struct AuthConfig {
    #[serde(default)]
    pub keys: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct CorsConfig {
    /// Allowed origins. Empty = allow any.
    #[serde(default)]
    pub allow_origins: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ModelEntry {
    pub provider: ProviderConfig,
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ProviderConfig {
    pub base_url: String,
    pub api_key: String,
    #[serde(default)]
    pub timeout: Option<u64>,
}

// ── Resolved (post-parse) configuration ──────────────────────────────────

#[derive(Debug, Clone)]
pub struct ResolvedProvider {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub timeout: Duration,
}

#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    pub listen: String,
    pub timeout: u64,
    pub auth_keys: HashSet<String>,
    pub cors_allow_origins: Vec<String>,
    pub tool_type_allowlist: Vec<String>,
    pub log_level: String,
    pub models: HashMap<String, ResolvedProvider>,
    pub model_names: Vec<String>,
    pub compact_encryption_key: String,
}

impl ResolvedConfig {
    pub fn auth_enabled(&self) -> bool {
        !self.auth_keys.is_empty()
    }
}

// ── Config loading pipeline ──────────────────────────────────────────────

pub fn load_config(path: &str) -> Result<ResolvedConfig, String> {
    let content =
        fs::read_to_string(Path::new(path)).map_err(|e| format!("Cannot read {path}: {e}"))?;
    let config: Config =
        serde_yaml::from_str(&content).map_err(|e| format!("Invalid YAML in {path}: {e}"))?;
    resolve_config(config)
}

/// Expands `$VAR` references to environment variable values.
fn resolve_env(raw: &str) -> String {
    if let Some(var) = raw.strip_prefix('$') {
        std::env::var(var).unwrap_or_else(|_| {
            tracing::warn!(env = %var, "Environment variable not set, using empty string");
            String::new()
        })
    } else {
        raw.to_string()
    }
}

/// Resolves env vars, fills defaults, and builds the runtime configuration.
fn resolve_config(config: Config) -> Result<ResolvedConfig, String> {
    let mut models = HashMap::new();
    let mut model_names = Vec::new();

    if config.models.is_empty() {
        return Err("No models configured in models".into());
    }

    let default_timeout = Duration::from_secs(config.server.timeout);

    for (logical_name, entry) in &config.models {
        let base_url = resolve_env(&entry.provider.base_url);
        let api_key = resolve_env(&entry.provider.api_key);
        let model = entry.model.clone().unwrap_or_else(|| logical_name.clone());
        let timeout = entry
            .provider
            .timeout
            .map(Duration::from_secs)
            .unwrap_or(default_timeout);

        models.insert(
            logical_name.clone(),
            ResolvedProvider {
                base_url,
                api_key,
                model,
                timeout,
            },
        );
        model_names.push(logical_name.clone());
    }

    Ok(ResolvedConfig {
        listen: config.server.listen,
        timeout: config.server.timeout,
        auth_keys: config.server.auth.keys.into_iter().collect::<HashSet<_>>(),
        cors_allow_origins: config.server.cors.allow_origins,
        tool_type_allowlist: config.server.tool_type_allowlist,
        log_level: config.server.log_level,
        models,
        model_names,
        compact_encryption_key: config.server.compact_encryption_key,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(yaml: &str) -> Result<ResolvedConfig, String> {
        let config: Config = serde_yaml::from_str(yaml).map_err(|e| format!("parse error: {e}"))?;
        resolve_config(config)
    }

    #[test]
    fn test_minimal_config() {
        let c = parse(
            "
models:
  gpt-4:
    provider:
      base_url: https://api.deepseek.com
      api_key: sk-abc
",
        )
        .unwrap();
        assert_eq!(c.models.len(), 1);
        let p = &c.models["gpt-4"];
        assert_eq!(p.base_url, "https://api.deepseek.com");
        assert_eq!(p.api_key, "sk-abc");
        assert_eq!(p.model, "gpt-4");
        assert_eq!(p.timeout, Duration::from_secs(600));
    }

    #[test]
    fn test_model_override() {
        let c = parse(
            "
models:
  gpt-5:
    provider:
      base_url: https://api.deepseek.com
      api_key: sk-abc
    model: deepseek-v4-pro
",
        )
        .unwrap();
        assert_eq!(c.models["gpt-5"].model, "deepseek-v4-pro");
    }

    #[test]
    fn test_provider_timeout_override() {
        let c = parse(
            "
server:
  timeout: 10
models:
  gpt-4:
    provider:
      base_url: https://api.deepseek.com
      api_key: sk-abc
      timeout: 60
",
        )
        .unwrap();
        assert_eq!(c.models["gpt-4"].timeout, Duration::from_secs(60));
    }

    #[test]
    fn test_empty_models_is_error() {
        assert!(parse("models: {}").is_err());
    }

    #[test]
    fn test_auth_enabled_with_keys() {
        let c = parse(
            "
models:
  gpt-4:
    provider:
      base_url: https://api.deepseek.com
      api_key: sk-abc
server:
  auth:
    keys: [key1, key2]
",
        )
        .unwrap();
        assert!(c.auth_enabled());
        assert_eq!(c.auth_keys.len(), 2);
    }

    #[test]
    fn test_auth_disabled_without_keys() {
        let c = parse(
            "
models:
  gpt-4:
    provider:
      base_url: https://api.deepseek.com
      api_key: sk-abc
",
        )
        .unwrap();
        assert!(!c.auth_enabled());
        assert!(c.auth_keys.is_empty());
    }

    #[test]
    fn test_full_config() {
        let c = parse(
            "
server:
  listen: '127.0.0.1:8080'
  timeout: 45
  auth:
    keys: [key1, key2]
  tool_type_allowlist: [function, web_search_preview]
  log_level: debug
  compact_encryption_key: abcd

models:
  gpt-4:
    provider:
      base_url: https://api.deepseek.com
      api_key: sk-abc
",
        )
        .unwrap();
        assert_eq!(c.listen, "127.0.0.1:8080");
        assert_eq!(c.timeout, 45);
        assert!(c.auth_enabled());
        assert_eq!(c.auth_keys.len(), 2);
        assert_eq!(
            c.tool_type_allowlist,
            vec!["function", "web_search_preview"]
        );
        assert_eq!(c.log_level, "debug");
        assert_eq!(c.compact_encryption_key, "abcd");
    }

    #[test]
    fn test_env_var_resolved() {
        unsafe { std::env::set_var("TEST_KEY", "resolved-value") };
        let c = parse(
            "
models:
  gpt-4:
    provider:
      base_url: https://api.deepseek.com
      api_key: $TEST_KEY
",
        )
        .unwrap();
        assert_eq!(c.models["gpt-4"].api_key, "resolved-value");
        unsafe { std::env::remove_var("TEST_KEY") };
    }

    #[test]
    fn test_env_var_unset_uses_empty() {
        unsafe { std::env::remove_var("MISSING_VAR") };
        let c = parse(
            "
models:
  gpt-4:
    provider:
      base_url: https://api.deepseek.com
      api_key: $MISSING_VAR
",
        )
        .unwrap();
        assert_eq!(c.models["gpt-4"].api_key, "");
    }

    #[test]
    fn test_custom_tool_allowlist() {
        let c = parse(
            "
server:
  tool_type_allowlist: [mcp, function]

models:
  gpt-4:
    provider:
      base_url: https://api.deepseek.com
      api_key: sk-abc
",
        )
        .unwrap();
        assert_eq!(c.tool_type_allowlist, vec!["mcp", "function"]);
    }

    #[test]
    fn test_tool_allowlist_defaults_to_function() {
        let c = parse(
            "
models:
  gpt-4:
    provider:
      base_url: https://api.deepseek.com
      api_key: sk-abc
",
        )
        .unwrap();
        assert_eq!(c.tool_type_allowlist, vec!["function"]);
    }

    #[test]
    fn test_missing_models_field_is_error() {
        assert!(parse("server:\n  listen: '0.0.0.0:3000'").is_err());
    }

    #[test]
    fn test_missing_server_uses_defaults() {
        let c = parse(
            "
models:
  gpt-4:
    provider:
      base_url: https://api.deepseek.com
      api_key: sk-abc
",
        )
        .unwrap();
        assert_eq!(c.listen, "0.0.0.0:3000");
        assert_eq!(c.timeout, 600);
        assert_eq!(c.models["gpt-4"].timeout, Duration::from_secs(600));
    }

    #[test]
    fn test_parse_error_invalid_yaml() {
        assert!(parse(": invalid").is_err());
    }

    #[test]
    fn test_parse_error_on_missing_provider() {
        assert!(
            parse(
                "
models:
  gpt-4:
    hello: world
"
            )
            .is_err()
        );
    }

    #[test]
    fn test_no_dollar_prefix_uses_raw_value() {
        let c = parse(
            "
models:
  gpt-4:
    provider:
      base_url: https://api.deepseek.com
      api_key: plain-text-key
",
        )
        .unwrap();
        assert_eq!(c.models["gpt-4"].api_key, "plain-text-key");
    }

    #[test]
    fn test_multiple_models_ordering() {
        let c = parse(
            "
models:
  a:
    provider:
      base_url: https://api.deepseek.com
      api_key: sk-1
  b:
    provider:
      base_url: https://api.deepseek.com
      api_key: sk-2
",
        )
        .unwrap();
        assert_eq!(c.models.len(), 2);
        assert_eq!(c.model_names.len(), 2);
        assert!(c.model_names.contains(&"a".into()));
        assert!(c.model_names.contains(&"b".into()));
    }
}
