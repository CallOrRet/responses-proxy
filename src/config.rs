use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub models: Vec<ModelEntry>,
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,
    #[serde(default = "default_request_timeout")]
    pub request_timeout: u64,
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default = "default_tool_allowlist")]
    pub tool_type_allowlist: Vec<String>,
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

fn default_log_level() -> String {
    "info".into()
}

fn default_tool_allowlist() -> Vec<String> {
    vec!["function".into()]
}

#[derive(Debug, Default, Deserialize)]
pub struct AuthConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub keys: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ModelEntry {
    pub model: String,
    pub provider: ProviderConfig,
    #[serde(default)]
    pub downstream_model: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ProviderConfig {
    pub base_url: String,
    pub api_key: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen_addr: default_listen_addr(),
            request_timeout: default_request_timeout(),
            auth: AuthConfig::default(),
            tool_type_allowlist: default_tool_allowlist(),
            log_level: default_log_level(),
        }
    }
}

fn default_listen_addr() -> String {
    "0.0.0.0:3000".into()
}

fn default_request_timeout() -> u64 {
    30
}

/// Resolved provider config with API key resolved from env if needed.
#[derive(Debug, Clone)]
pub struct ResolvedProvider {
    pub base_url: String,
    pub api_key: String,
    pub downstream_model: String,
}

/// Fully resolved config with model lookup index.
#[derive(Debug)]
pub struct ResolvedConfig {
    pub listen_addr: String,
    pub request_timeout: u64,
    pub auth_enabled: bool,
    pub auth_keys: HashSet<String>,
    pub tool_type_allowlist: Vec<String>,
    pub log_level: String,
    pub models: HashMap<String, ResolvedProvider>,
    /// Ordered list of model names for /v1/models
    pub model_names: Vec<String>,
}

pub fn load_config(path: &str) -> Result<ResolvedConfig, String> {
    let config_path = Path::new(path);
    if !config_path.exists() {
        return Err(format!("Config file not found: {}", path));
    }

    let content = fs::read_to_string(config_path)
        .map_err(|e| format!("Failed to read config file: {}", e))?;

    let config: Config = serde_yaml::from_str(&content)
        .map_err(|e| format!("Failed to parse config.yaml: {}", e))?;

    resolve_config(config)
}

/// Resolve a config value. If it starts with `$`, reads from the named environment variable.
fn resolve_env(raw: &str) -> String {
    if let Some(var) = raw.strip_prefix('$') {
        return std::env::var(var).unwrap_or_else(|_| {
            tracing::warn!("Environment variable {} not set, using empty value", var);
            String::new()
        });
    }
    raw.to_string()
}

fn resolve_config(config: Config) -> Result<ResolvedConfig, String> {
    let mut models = HashMap::new();
    let mut model_names = Vec::new();

    if config.models.is_empty() {
        return Err("No models configured in models".into());
    }

    for entry in &config.models {
        let base_url = resolve_env(&entry.provider.base_url);
        let api_key = resolve_env(&entry.provider.api_key);
        let downstream_model = entry
            .downstream_model
            .clone()
            .unwrap_or_else(|| entry.model.clone());

        models.insert(
            entry.model.clone(),
            ResolvedProvider {
                base_url,
                api_key,
                downstream_model,
            },
        );
        model_names.push(entry.model.clone());
    }

    Ok(ResolvedConfig {
        listen_addr: config.server.listen_addr,
        request_timeout: config.server.request_timeout,
        auth_enabled: config.server.auth.enabled,
        auth_keys: config.server.auth.keys.into_iter().collect::<HashSet<_>>(),
        tool_type_allowlist: config.server.tool_type_allowlist,
        log_level: config.server.log_level,
        models,
        model_names,
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
        let yaml = r#"
models:
  - model: deepseek-v4-pro
    provider:
      base_url: https://api.deepseek.com
      api_key: sk-abc
"#;
        let c = parse(yaml).unwrap();
        assert_eq!(c.listen_addr, "0.0.0.0:3000");
        assert_eq!(c.request_timeout, 30);
        assert!(!c.auth_enabled);
        assert!(c.auth_keys.is_empty());
        assert_eq!(c.tool_type_allowlist, vec!["function"]);
        assert_eq!(c.model_names, vec!["deepseek-v4-pro"]);
        let p = c.models.get("deepseek-v4-pro").unwrap();
        assert_eq!(p.base_url, "https://api.deepseek.com");
        assert_eq!(p.api_key, "sk-abc");
        assert_eq!(p.downstream_model, "deepseek-v4-pro");
    }

    #[test]
    fn test_full_config() {
        let yaml = r#"
server:
  listen_addr: "127.0.0.1:8080"
  request_timeout: 60
  auth:
    enabled: true
    keys:
      - sk-secret-1
      - sk-secret-2
  tool_type_allowlist:
    - function
    - web_search_preview

models:
  - model: my-model
    provider:
      base_url: https://example.com
      api_key: sk-xyz
    downstream_model: their-model
"#;
        let c = parse(yaml).unwrap();
        assert_eq!(c.listen_addr, "127.0.0.1:8080");
        assert_eq!(c.request_timeout, 60);
        assert!(c.auth_enabled);
        assert_eq!(
            c.auth_keys,
            ["sk-secret-1".into(), "sk-secret-2".into()]
                .into_iter()
                .collect::<HashSet<_>>()
        );
        assert_eq!(
            c.tool_type_allowlist,
            vec!["function", "web_search_preview"]
        );
        let p = c.models.get("my-model").unwrap();
        assert_eq!(p.base_url, "https://example.com");
        assert_eq!(p.api_key, "sk-xyz");
        assert_eq!(p.downstream_model, "their-model");
    }

    #[test]
    fn test_missing_server_uses_defaults() {
        let yaml = r#"
models:
  - model: m
    provider:
      base_url: https://x.com
      api_key: k
"#;
        let c = parse(yaml).unwrap();
        assert_eq!(c.listen_addr, "0.0.0.0:3000");
        assert_eq!(c.request_timeout, 30);
        assert!(!c.auth_enabled);
    }

    #[test]
    fn test_auth_optional_fields_default() {
        let yaml = r#"
server:
  auth: {}
models:
  - model: m
    provider:
      base_url: https://x.com
      api_key: k
"#;
        let c = parse(yaml).unwrap();
        assert!(!c.auth_enabled);
        assert!(c.auth_keys.is_empty());
    }

    #[test]
    fn test_tool_allowlist_defaults_to_function() {
        let yaml = r#"
models:
  - model: m
    provider:
      base_url: https://x.com
      api_key: k
"#;
        let c = parse(yaml).unwrap();
        assert_eq!(c.tool_type_allowlist, vec!["function"]);
    }

    #[test]
    fn test_multiple_models_ordering() {
        let yaml = r#"
models:
  - model: b
    provider:
      base_url: https://b.com
      api_key: kb
  - model: a
    provider:
      base_url: https://a.com
      api_key: ka
"#;
        let c = parse(yaml).unwrap();
        assert_eq!(c.model_names, vec!["b", "a"]);
        assert!(c.models.contains_key("b"));
        assert!(c.models.contains_key("a"));
        assert_eq!(c.models["b"].base_url, "https://b.com");
        assert_eq!(c.models["a"].base_url, "https://a.com");
    }

    #[test]
    fn test_downstream_model_defaults_to_model() {
        let yaml = r#"
models:
  - model: gpt-4
    provider:
      base_url: https://x.com
      api_key: k
"#;
        let c = parse(yaml).unwrap();
        assert_eq!(c.models["gpt-4"].downstream_model, "gpt-4");
    }

    #[test]
    fn test_env_var_resolved() {
        unsafe {
            std::env::set_var("TEST_API_KEY", "sk-from-env");
            std::env::set_var("TEST_BASE_URL", "https://env.example.com");
        }

        let yaml = r#"
models:
  - model: m
    provider:
      base_url: $TEST_BASE_URL
      api_key: $TEST_API_KEY
"#;
        let c = parse(yaml).unwrap();
        let p = c.models.get("m").unwrap();
        assert_eq!(p.base_url, "https://env.example.com");
        assert_eq!(p.api_key, "sk-from-env");
    }

    #[test]
    fn test_env_var_unset_uses_empty() {
        unsafe {
            std::env::remove_var("MISSING_VAR");
        }

        let yaml = r#"
models:
  - model: m
    provider:
      base_url: https://x.com
      api_key: $MISSING_VAR
"#;
        let c = parse(yaml).unwrap();
        assert_eq!(c.models["m"].api_key, "");
    }

    #[test]
    fn test_no_dollar_prefix_uses_raw_value() {
        let yaml = r#"
models:
  - model: m
    provider:
      base_url: https://api.example.com/v1
      api_key: sk-plain-text-key
"#;
        let c = parse(yaml).unwrap();
        let p = c.models.get("m").unwrap();
        assert_eq!(p.base_url, "https://api.example.com/v1");
        assert_eq!(p.api_key, "sk-plain-text-key");
    }

    #[test]
    fn test_empty_models_is_error() {
        let yaml = r#"
models: []
"#;
        let err = parse(yaml).unwrap_err();
        assert!(err.contains("No models configured"));
    }

    #[test]
    fn test_missing_models_field_is_error() {
        let yaml = r#"
server:
  listen_addr: "0.0.0.0:3000"
"#;
        let err = parse(yaml).unwrap_err();
        assert!(err.contains("No models configured"));
    }

    #[test]
    fn test_parse_error_on_missing_provider() {
        let yaml = r#"
models:
  - model: m
"#;
        // provider is required, so YAML deser should fail
        let result = parse(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_error_invalid_yaml() {
        let result = serde_yaml::from_str::<Config>("{{{ bad yaml");
        assert!(result.is_err());
    }

    #[test]
    fn test_custom_tool_allowlist() {
        let yaml = r#"
server:
  tool_type_allowlist:
    - function
    - web_search_preview
    - image_generation
models:
  - model: m
    provider:
      base_url: https://x.com
      api_key: k
"#;
        let c = parse(yaml).unwrap();
        assert_eq!(
            c.tool_type_allowlist,
            vec!["function", "web_search_preview", "image_generation"]
        );
    }
}
