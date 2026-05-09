use serde::Deserialize;
use std::collections::HashMap;
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
    #[serde(default = "default_timeout")]
    pub request_timeout_secs: u64,
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default = "default_tool_allowlist")]
    pub tool_type_allowlist: Vec<String>,
}

fn default_tool_allowlist() -> Vec<String> {
    vec!["function".into()]
}

#[derive(Debug, Deserialize)]
pub struct AuthConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub keys: Vec<String>,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            keys: vec![],
        }
    }
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
            request_timeout_secs: default_timeout(),
            auth: AuthConfig::default(),
            tool_type_allowlist: default_tool_allowlist(),
        }
    }
}

fn default_listen_addr() -> String {
    "0.0.0.0:3000".into()
}

fn default_timeout() -> u64 {
    120
}

/// Resolved provider config with API key resolved from env if needed.
pub struct ResolvedProvider {
    pub base_url: String,
    pub api_key: String,
    pub downstream_model: String,
}

/// Fully resolved config with model lookup index.
pub struct ResolvedConfig {
    pub listen_addr: String,
    pub request_timeout_secs: u64,
    pub auth_enabled: bool,
    pub auth_keys: Vec<String>,
    pub tool_type_allowlist: Vec<String>,
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

fn resolve_api_key(raw: &str) -> String {
    if let Some(env_var) = raw.strip_prefix("os.environ/") {
        std::env::var(env_var).unwrap_or_else(|_| {
            tracing::warn!("Environment variable {} not set, using empty key", env_var);
            String::new()
        })
    } else {
        raw.to_string()
    }
}

fn resolve_config(config: Config) -> Result<ResolvedConfig, String> {
    let mut models = HashMap::new();
    let mut model_names = Vec::new();

    if config.models.is_empty() {
        return Err("No models configured in models".into());
    }

    for entry in &config.models {
        let api_key = resolve_api_key(&entry.provider.api_key);
        let downstream_model = entry
            .downstream_model
            .clone()
            .unwrap_or_else(|| entry.model.clone());

        models.insert(
            entry.model.clone(),
            ResolvedProvider {
                base_url: entry.provider.base_url.clone(),
                api_key,
                downstream_model,
            },
        );
        model_names.push(entry.model.clone());
    }

    Ok(ResolvedConfig {
        listen_addr: config.server.listen_addr,
        request_timeout_secs: config.server.request_timeout_secs,
        auth_enabled: config.server.auth.enabled,
        auth_keys: config.server.auth.keys,
        tool_type_allowlist: config.server.tool_type_allowlist,
        models,
        model_names,
    })
}
