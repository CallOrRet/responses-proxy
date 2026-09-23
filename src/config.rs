use crate::types::ReasoningEffort;
use serde::Deserialize;
use serde::de::{self, MapAccess, Visitor};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::time::Duration;

// ── Raw configuration (from YAML) ────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub models: HashMap<String, ModelEntry>,
    #[serde(default)]
    pub rewrites: HashMap<String, RewriteProfile>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ServerConfig {
    #[serde(default = "default_listen")]
    pub listen: String,
    #[serde(default = "default_timeout")]
    pub timeout: u64,
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub cors: CorsConfig,
    #[serde(default = "default_allowed_tool_types")]
    pub allowed_tool_types: Vec<String>,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default)]
    pub compact_encryption_key: String,
    #[serde(default = "default_max_body_mb")]
    pub max_body_mb: usize,
}

fn default_max_body_mb() -> usize {
    100
}

fn default_log_level() -> String {
    "info".into()
}

fn default_allowed_tool_types() -> Vec<String> {
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
            allowed_tool_types: default_allowed_tool_types(),
            log_level: default_log_level(),
            compact_encryption_key: String::new(),
            max_body_mb: default_max_body_mb(),
        }
    }
}

// ── Sub-config sections ──────────────────────────────────────────────────

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthConfig {
    #[serde(default)]
    pub keys: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct CorsConfig {
    /// Allowed origins. Empty = allow any.
    #[serde(default)]
    pub allow_origins: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct ModelEntry {
    pub provider: ProviderConfig,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub rewrite: Option<RewriteEntry>,
    #[serde(default)]
    pub history: Option<HistoryConfig>,
    /// Reasoning tiers advertised to Codex for this model. Controls what the
    /// Codex `/model` picker offers; omit to use the built-in default set.
    #[serde(default)]
    pub reasoning: Option<ReasoningConfig>,
    /// Whether the upstream can stream structured output (`response_format`
    /// json_schema/json_object with `stream: true`). Some gateways reject that
    /// combination; set `false` and the proxy fetches the response
    /// non-streamed, then re-emits it to the client as a single SSE burst.
    /// Defaults to `true` (pass streaming through unchanged).
    #[serde(default, rename = "stream-structured-output")]
    pub stream_structured_output: Option<bool>,
    /// Maximum number of tools forwarded to the upstream in a single request.
    /// Some gateways reject requests carrying more than a fixed number of tools;
    /// Codex code mode can flatten a large MCP/app-tool registry into hundreds of
    /// functions, so cap it here. When the converted tool list exceeds this, the
    /// leading tools (Codex lists its core coding tools first) are kept and the
    /// overflow is dropped. `0` (the default) disables the cap.
    #[serde(default, rename = "max-tools")]
    pub max_tools: usize,
}

/// Per-model history controls. `max-input-chars` caps the total size of the
/// converted Chat request; when the replayed history exceeds it, the oldest
/// tool outputs are truncated until the request fits.
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct HistoryConfig {
    #[serde(default)]
    pub max_input_chars: Option<usize>,
    #[serde(default = "default_max_input_messages")]
    pub max_input_messages: usize,
    /// Context window (in tokens) advertised to Codex via `/v1/models`. Codex
    /// keys its client-side auto-compaction off this size. When set it
    /// overrides whatever the upstream advertises; when unset the proxy trusts
    /// the upstream's own context length (or omits it, letting Codex fall back
    /// to its bundled default).
    #[serde(default)]
    pub context_window: Option<i64>,
}

fn default_max_input_messages() -> usize {
    1000
}

/// Per-model reasoning tiers advertised to Codex via the native `/v1/models`
/// catalog. Codex builds its `/model` picker verbatim from `levels` (in order),
/// so a model that advertises only `medium` shows only Medium. `default` anchors
/// the picker's default selection.
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ReasoningConfig {
    /// Ordered reasoning tiers to advertise. Order is preserved in the picker.
    #[serde(default)]
    pub levels: Option<Vec<ReasoningEffort>>,
    /// Default reasoning tier the picker anchors to.
    #[serde(default, rename = "default")]
    pub default_level: Option<ReasoningEffort>,
}

/// Built-in tiers advertised when a model has no explicit `reasoning` block.
/// `max`/`ultra` only exist on the provider's `/v1/messages` endpoint, so a
/// model routing them through Chat Completions must clamp them via a rewrite.
fn default_reasoning_levels() -> Vec<ReasoningEffort> {
    vec![
        ReasoningEffort::Low,
        ReasoningEffort::Medium,
        ReasoningEffort::High,
        ReasoningEffort::Xhigh,
        ReasoningEffort::Max,
        ReasoningEffort::Ultra,
    ]
}

/// Resolve a model's advertised reasoning tiers and default, applying built-in
/// defaults when the block (or its fields) is absent or empty.
fn resolve_reasoning(cfg: Option<&ReasoningConfig>) -> (Vec<ReasoningEffort>, ReasoningEffort) {
    let levels = cfg
        .and_then(|c| c.levels.clone())
        .filter(|l| !l.is_empty())
        .unwrap_or_else(default_reasoning_levels);
    let default_level = cfg
        .and_then(|c| c.default_level.clone())
        .unwrap_or(ReasoningEffort::Medium);
    (levels, default_level)
}

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum RewriteEntry {
    Ref(String),
    Inline(RewriteProfile),
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ProviderConfig {
    pub base_url: String,
    pub api_key: String,
    #[serde(default)]
    pub timeout: Option<u64>,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct RewriteProfile {
    pub responses_in: RewriteConfig,
    pub chat_out: RewriteConfig,
    pub chat_in: RewriteConfig,
    pub responses_out: RewriteConfig,
}

impl<'de> Deserialize<'de> for RewriteProfile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum RawRewriteProfile {
            Many(Vec<RewriteStageConfig>),
            One(RewriteStageConfig),
        }

        let raw = RawRewriteProfile::deserialize(deserializer)?;
        let stages = match raw {
            RawRewriteProfile::Many(stages) => stages,
            RawRewriteProfile::One(stage) => vec![stage],
        };

        let mut profile = RewriteProfile::default();
        for stage in stages {
            let target = match stage.at.as_str() {
                "response-in" => &mut profile.responses_in,
                "chat-out" => &mut profile.chat_out,
                "chat-in" => &mut profile.chat_in,
                "response-out" => &mut profile.responses_out,
                other => {
                    return Err(de::Error::custom(format!(
                        "unknown rewrite stage '{other}', expected one of response-in, chat-out, chat-in, response-out"
                    )));
                }
            };
            if !target.is_empty() {
                return Err(de::Error::custom(format!(
                    "duplicate rewrite stage '{}'",
                    stage.at
                )));
            }
            validate_rewrite_stage(&stage.at, &stage.rewrite).map_err(de::Error::custom)?;
            *target = stage.rewrite;
        }
        Ok(profile)
    }
}

#[derive(Debug, Clone, PartialEq)]
struct RewriteStageConfig {
    at: String,
    rewrite: RewriteConfig,
}

impl<'de> Deserialize<'de> for RewriteStageConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct RewriteStageConfigVisitor;

        impl<'de> Visitor<'de> for RewriteStageConfigVisitor {
            type Value = RewriteStageConfig;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a rewrite stage config map with an at field")
            }

            fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut at = None;
                let mut steps = Vec::new();
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "at" => {
                            at = Some(map.next_value()?);
                        }
                        "reset" => {
                            let value = map.next_value::<OrderedJsonMap>()?;
                            steps.push(RewriteStep::Reset(value.0));
                        }
                        "rename" => {
                            let value = map.next_value::<OrderedStringMap>()?;
                            steps.push(RewriteStep::Rename(value.0));
                        }
                        "remove" => {
                            steps.push(RewriteStep::Remove(map.next_value()?));
                        }
                        "replace" => {
                            let value = map.next_value::<OrderedReplaceMap>()?;
                            steps.push(RewriteStep::Replace(value.0));
                        }
                        other => {
                            return Err(de::Error::unknown_field(
                                other,
                                &["at", "reset", "rename", "remove", "replace"],
                            ));
                        }
                    }
                }
                let at = at.ok_or_else(|| de::Error::missing_field("at"))?;
                Ok(RewriteStageConfig {
                    at,
                    rewrite: RewriteConfig { steps },
                })
            }
        }

        deserializer.deserialize_map(RewriteStageConfigVisitor)
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct RewriteConfig {
    pub steps: Vec<RewriteStep>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RewriteStep {
    Reset(Vec<(String, serde_json::Value)>),
    Rename(Vec<(String, String)>),
    Remove(Vec<String>),
    Replace(Vec<(String, Vec<ReplaceRule>)>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReplaceRule {
    pub condition: RewriteCondition,
    pub set: Option<serde_json::Value>,
    pub with: Vec<(String, serde_json::Value)>,
}

#[derive(Debug, Clone)]
pub enum RewriteCondition {
    Regex {
        pattern: String,
        regex: regex::Regex,
    },
    Any(Vec<RewriteCondition>),
    Value(serde_json::Value),
}

impl PartialEq for RewriteCondition {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Regex { pattern: a, .. }, Self::Regex { pattern: b, .. }) => a == b,
            (Self::Any(a), Self::Any(b)) => a == b,
            (Self::Value(a), Self::Value(b)) => a == b,
            _ => false,
        }
    }
}

impl RewriteCondition {
    pub(crate) fn from_value(value: serde_json::Value) -> Result<Self, String> {
        match value {
            serde_json::Value::String(pattern) => {
                let regex = regex::Regex::new(&pattern)
                    .map_err(|e| format!("invalid rewrite if regex '{pattern}': {e}"))?;
                Ok(Self::Regex { pattern, regex })
            }
            serde_json::Value::Array(candidates) => {
                let mut compiled = Vec::with_capacity(candidates.len());
                for candidate in candidates {
                    compiled.push(Self::from_value(candidate)?);
                }
                Ok(Self::Any(compiled))
            }
            value => Ok(Self::Value(value)),
        }
    }
}

impl<'de> Deserialize<'de> for ReplaceRule {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct RawReplaceRule {
            #[serde(rename = "if")]
            condition: serde_json::Value,
            #[serde(default)]
            set: Option<serde_json::Value>,
            #[serde(rename = "with", default)]
            with_paths: Option<OrderedJsonMap>,
        }

        let raw = RawReplaceRule::deserialize(deserializer)?;
        let with = raw.with_paths.map(|paths| paths.0).unwrap_or_default();
        if raw.set.is_none() && with.is_empty() {
            return Err(de::Error::custom("replace rule must contain set or with"));
        }
        let condition = RewriteCondition::from_value(raw.condition).map_err(de::Error::custom)?;
        Ok(Self {
            condition,
            set: raw.set,
            with,
        })
    }
}

impl<'de> Deserialize<'de> for RewriteConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct RewriteConfigVisitor;

        impl<'de> Visitor<'de> for RewriteConfigVisitor {
            type Value = RewriteConfig;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a rewrite config map")
            }

            fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut steps = Vec::new();
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "reset" => {
                            let value = map.next_value::<OrderedJsonMap>()?;
                            steps.push(RewriteStep::Reset(value.0));
                        }
                        "rename" => {
                            let value = map.next_value::<OrderedStringMap>()?;
                            steps.push(RewriteStep::Rename(value.0));
                        }
                        "remove" => {
                            steps.push(RewriteStep::Remove(map.next_value()?));
                        }
                        "replace" => {
                            let value = map.next_value::<OrderedReplaceMap>()?;
                            steps.push(RewriteStep::Replace(value.0));
                        }
                        other => {
                            return Err(de::Error::unknown_field(
                                other,
                                &["reset", "rename", "remove", "replace"],
                            ));
                        }
                    }
                }
                Ok(RewriteConfig { steps })
            }
        }

        deserializer.deserialize_map(RewriteConfigVisitor)
    }
}

impl RewriteConfig {
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}

fn validate_rewrite_stage(stage: &str, rewrite: &RewriteConfig) -> Result<(), String> {
    if rewrite.is_empty() {
        return Err(format!(
            "rewrite stage '{stage}' must contain at least one rule"
        ));
    }

    for step in &rewrite.steps {
        if let RewriteStep::Replace(rules) = step {
            for (path, path_rules) in rules {
                let normalized_path = normalize_config_path(path);
                for rule in path_rules {
                    if rule.set.is_some()
                        && rule.with.iter().any(|(with_path, _)| {
                            normalize_config_path(with_path) == normalized_path
                        })
                    {
                        return Err(format!(
                            "replace rule for '{path}' cannot set the current path again in with"
                        ));
                    }
                }
            }
        }
    }

    Ok(())
}

fn normalize_config_path(path: &str) -> String {
    if path.is_empty() || path.starts_with('/') {
        return path.to_string();
    }
    format!(
        "/{}",
        path.split('.')
            .map(|token| token.replace('~', "~0").replace('/', "~1"))
            .collect::<Vec<_>>()
            .join("/")
    )
}

struct OrderedJsonMap(Vec<(String, serde_json::Value)>);
struct OrderedStringMap(Vec<(String, String)>);
struct OrderedReplaceMap(Vec<(String, Vec<ReplaceRule>)>);

impl<'de> Deserialize<'de> for OrderedJsonMap {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct OrderedJsonMapVisitor;

        impl<'de> Visitor<'de> for OrderedJsonMapVisitor {
            type Value = OrderedJsonMap;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("an ordered JSON value map")
            }

            fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut pairs = Vec::new();
                while let Some(key) = map.next_key::<String>()? {
                    pairs.push((key, map.next_value()?));
                }
                Ok(OrderedJsonMap(pairs))
            }
        }

        deserializer.deserialize_map(OrderedJsonMapVisitor)
    }
}

impl<'de> Deserialize<'de> for OrderedStringMap {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct OrderedStringMapVisitor;

        impl<'de> Visitor<'de> for OrderedStringMapVisitor {
            type Value = OrderedStringMap;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("an ordered string map")
            }

            fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut pairs = Vec::new();
                while let Some(key) = map.next_key::<String>()? {
                    pairs.push((key, map.next_value()?));
                }
                Ok(OrderedStringMap(pairs))
            }
        }

        deserializer.deserialize_map(OrderedStringMapVisitor)
    }
}

impl<'de> Deserialize<'de> for OrderedReplaceMap {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct OrderedReplaceMapVisitor;

        impl<'de> Visitor<'de> for OrderedReplaceMapVisitor {
            type Value = OrderedReplaceMap;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("an ordered replace rule map")
            }

            fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut pairs = Vec::new();
                while let Some(key) = map.next_key::<String>()? {
                    pairs.push((key, map.next_value::<Vec<ReplaceRule>>()?));
                }
                Ok(OrderedReplaceMap(pairs))
            }
        }

        deserializer.deserialize_map(OrderedReplaceMapVisitor)
    }
}

// ── Resolved (post-parse) configuration ──────────────────────────────────

#[derive(Debug, Clone)]
pub struct ResolvedProvider {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub timeout: Duration,
    pub rewrite: RewriteProfile,
    /// Total character ceiling for the converted Chat request, if configured.
    pub max_input_chars: Option<usize>,
    /// Maximum number of messages in the converted Chat request. 0 = unlimited.
    pub max_input_messages: usize,
    /// Maximum number of tools forwarded to the upstream. `0` = no cap.
    pub max_tools: usize,
    /// Whether the upstream can stream structured output. When `false`, the
    /// proxy buffers a non-streamed upstream response and replays it as SSE.
    pub stream_structured_output: bool,
    /// Context window (tokens) to advertise to Codex, overriding the upstream's
    /// own value. `None` means trust the upstream (or Codex's bundled default).
    pub context_window: Option<i64>,
    /// Reasoning tiers advertised to Codex, in picker order. Never empty.
    pub reasoning_levels: Vec<ReasoningEffort>,
    /// Default reasoning tier advertised to Codex.
    pub default_reasoning_level: ReasoningEffort,
}

#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    pub listen: String,
    pub timeout: u64,
    pub auth_keys: HashSet<String>,
    pub cors_allow_origins: Vec<String>,
    pub allowed_tool_types: Vec<String>,
    pub log_level: String,
    pub models: HashMap<String, ResolvedProvider>,
    pub model_names: Vec<String>,
    pub compact_encryption_key: String,
    /// Maximum incoming request body size in bytes (after decompression).
    pub max_body_bytes: usize,
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
    let mut used_rewrites = HashSet::new();

    if config.models.is_empty() {
        return Err("No models configured in models".into());
    }

    let default_timeout = Duration::from_secs(config.server.timeout);

    for (logical_name, entry) in &config.models {
        let base_url = resolve_env(&entry.provider.base_url);
        let api_key = resolve_env(&entry.provider.api_key);
        let model = entry.model.clone().unwrap_or_else(|| logical_name.clone());
        let rewrite = resolve_rewrite(&config.rewrites, &entry.rewrite, &mut used_rewrites)?;
        let timeout = entry
            .provider
            .timeout
            .map(Duration::from_secs)
            .unwrap_or(default_timeout);
        let max_input_chars = entry.history.as_ref().and_then(|h| h.max_input_chars);
        let max_input_messages = entry
            .history
            .as_ref()
            .map(|h| h.max_input_messages)
            .unwrap_or_else(default_max_input_messages);
        let stream_structured_output = entry.stream_structured_output.unwrap_or(true);
        let context_window = entry.history.as_ref().and_then(|h| h.context_window);
        let (reasoning_levels, default_reasoning_level) =
            resolve_reasoning(entry.reasoning.as_ref());

        models.insert(
            logical_name.clone(),
            ResolvedProvider {
                base_url,
                api_key,
                model,
                timeout,
                rewrite,
                max_input_chars,
                max_input_messages,
                max_tools: entry.max_tools,
                stream_structured_output,
                context_window,
                reasoning_levels,
                default_reasoning_level,
            },
        );
        model_names.push(logical_name.clone());
    }

    for name in config.rewrites.keys() {
        if !used_rewrites.contains(name) {
            return Err(format!("Unused rewrite profile: {name}"));
        }
    }

    Ok(ResolvedConfig {
        listen: config.server.listen,
        timeout: config.server.timeout,
        auth_keys: config.server.auth.keys.into_iter().collect::<HashSet<_>>(),
        cors_allow_origins: config.server.cors.allow_origins,
        allowed_tool_types: config.server.allowed_tool_types,
        log_level: config.server.log_level,
        models,
        model_names,
        compact_encryption_key: config.server.compact_encryption_key,
        max_body_bytes: config.server.max_body_mb * 1024 * 1024,
    })
}

fn resolve_rewrite(
    rewrites: &HashMap<String, RewriteProfile>,
    entry: &Option<RewriteEntry>,
    used_rewrites: &mut HashSet<String>,
) -> Result<RewriteProfile, String> {
    match entry {
        None => Ok(RewriteProfile::default()),
        Some(RewriteEntry::Inline(profile)) => Ok(profile.clone()),
        Some(RewriteEntry::Ref(name)) => {
            let profile = rewrites
                .get(name)
                .cloned()
                .ok_or_else(|| format!("Unknown rewrite profile: {name}"))?;
            used_rewrites.insert(name.clone());
            Ok(profile)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(yaml: &str) -> Result<ResolvedConfig, String> {
        let config: Config = serde_yaml::from_str(yaml).map_err(|e| format!("parse error: {e}"))?;
        resolve_config(config)
    }

    #[test]
    fn test_example_configs_parse() {
        parse(include_str!("../config.yaml")).unwrap();
        parse(include_str!("../config.zh-CN.yaml")).unwrap();
        parse(include_str!("../deepseek.config.yaml")).unwrap();
    }

    #[test]
    fn test_minimal_config() {
        let c = parse(
            "
models:
  gpt-4:
    provider:
      base-url: https://api.deepseek.com
      api-key: sk-abc
",
        )
        .unwrap();
        assert_eq!(c.models.len(), 1);
        let p = &c.models["gpt-4"];
        assert_eq!(p.base_url, "https://api.deepseek.com");
        assert_eq!(p.api_key, "sk-abc");
        assert_eq!(p.model, "gpt-4");
        assert_eq!(p.timeout, Duration::from_secs(600));
        assert_eq!(p.rewrite, RewriteProfile::default());
    }

    #[test]
    fn test_rewrite_profile_reference() {
        let c = parse(
            "
models:
  first:
    provider:
      base-url: https://api.deepseek.com
      api-key: sk-abc
    rewrite: shared
  second:
    provider:
      base-url: https://api.deepseek.com
      api-key: sk-def
    rewrite: shared
rewrites:
  shared:
    at: chat-out
    rename:
      max_completion_tokens: max_tokens
",
        )
        .unwrap();

        assert_eq!(
            c.models["first"].rewrite.chat_out.steps,
            vec![RewriteStep::Rename(vec![(
                "max_completion_tokens".into(),
                "max_tokens".into()
            )])]
        );
        assert_eq!(
            c.models["second"].rewrite.chat_out.steps,
            c.models["first"].rewrite.chat_out.steps
        );
    }

    #[test]
    fn test_unknown_rewrite_profile_reference_is_error() {
        let err = parse(
            "
models:
  first:
    provider:
      base-url: https://api.deepseek.com
      api-key: sk-abc
    rewrite: missing
",
        )
        .unwrap_err();

        assert!(err.contains("Unknown rewrite profile: missing"));
    }

    #[test]
    fn test_unused_rewrite_profile_is_error() {
        let err = parse(
            "
models:
  first:
    provider:
      base-url: https://api.deepseek.com
      api-key: sk-abc
rewrites:
  unused:
    at: chat-out
    rename:
      max_completion_tokens: max_tokens
",
        )
        .unwrap_err();

        assert!(err.contains("Unused rewrite profile: unused"));
    }

    #[test]
    fn test_unknown_config_field_is_error() {
        let err = parse(
            "
server:
  log_level: debug
models:
  gpt-4:
    provider:
      base-url: https://api.deepseek.com
      api-key: sk-abc
",
        )
        .unwrap_err();

        assert!(err.contains("unknown field"));
    }

    #[test]
    fn test_empty_rewrite_stage_is_error() {
        let err = parse(
            "
models:
  gpt-4:
    provider:
      base-url: https://api.deepseek.com
      api-key: sk-abc
    rewrite:
      at: chat-out
",
        )
        .unwrap_err();

        assert!(!err.is_empty());
    }

    #[test]
    fn test_invalid_rewrite_if_regex_is_error() {
        let err = parse(
            "
models:
  gpt-4:
    provider:
      base-url: https://api.deepseek.com
      api-key: sk-abc
    rewrite:
      at: chat-out
      replace:
        reasoning_effort:
          - if: '('
            set: max
",
        )
        .unwrap_err();

        assert!(!err.is_empty());
    }

    #[test]
    fn test_replace_set_and_with_cannot_write_same_path() {
        let err = parse(
            "
models:
  gpt-4:
    provider:
      base-url: https://api.deepseek.com
      api-key: sk-abc
    rewrite:
      at: chat-out
      replace:
        reasoning_effort:
          - if: high
            set: max
            with:
              reasoning_effort: high
",
        )
        .unwrap_err();

        assert!(!err.is_empty());
    }

    #[test]
    fn test_model_override() {
        let c = parse(
            "
models:
  gpt-5:
    provider:
      base-url: https://api.deepseek.com
      api-key: sk-abc
    model: deepseek-v4-pro
",
        )
        .unwrap();
        assert_eq!(c.models["gpt-5"].model, "deepseek-v4-pro");
    }

    #[test]
    fn test_history_and_body_defaults() {
        let c = parse(
            "
models:
  gpt-4:
    provider:
      base-url: https://api.deepseek.com
      api-key: sk-abc
",
        )
        .unwrap();
        assert_eq!(c.models["gpt-4"].max_input_messages, 1000);
        assert_eq!(c.max_body_bytes, 100 * 1024 * 1024);
    }

    #[test]
    fn test_history_and_body_overrides() {
        let c = parse(
            "
server:
  max-body-mb: 25
models:
  gpt-4:
    provider:
      base-url: https://api.deepseek.com
      api-key: sk-abc
    history:
      max-input-messages: 50
      max-input-chars: 200000
",
        )
        .unwrap();
        assert_eq!(c.models["gpt-4"].max_input_messages, 50);
        assert_eq!(c.models["gpt-4"].max_input_chars, Some(200000));
        assert_eq!(c.max_body_bytes, 25 * 1024 * 1024);
    }

    #[test]
    fn test_reasoning_defaults_when_absent() {
        let c = parse(
            "
models:
  gpt-4:
    provider:
      base-url: https://api.deepseek.com
      api-key: sk-abc
",
        )
        .unwrap();
        let p = &c.models["gpt-4"];
        assert_eq!(
            p.reasoning_levels,
            vec![
                ReasoningEffort::Low,
                ReasoningEffort::Medium,
                ReasoningEffort::High,
                ReasoningEffort::Xhigh,
                ReasoningEffort::Max,
                ReasoningEffort::Ultra,
            ]
        );
        assert_eq!(p.default_reasoning_level, ReasoningEffort::Medium);
    }

    #[test]
    fn test_reasoning_override() {
        let c = parse(
            "
models:
  gpt-4:
    provider:
      base-url: https://api.deepseek.com
      api-key: sk-abc
    reasoning:
      default: xhigh
      levels: [low, high, xhigh]
",
        )
        .unwrap();
        let p = &c.models["gpt-4"];
        assert_eq!(
            p.reasoning_levels,
            vec![
                ReasoningEffort::Low,
                ReasoningEffort::High,
                ReasoningEffort::Xhigh,
            ]
        );
        assert_eq!(p.default_reasoning_level, ReasoningEffort::Xhigh);
    }

    #[test]
    fn test_reasoning_empty_levels_falls_back_to_default() {
        let c = parse(
            "
models:
  gpt-4:
    provider:
      base-url: https://api.deepseek.com
      api-key: sk-abc
    reasoning:
      levels: []
",
        )
        .unwrap();
        let p = &c.models["gpt-4"];
        assert_eq!(p.reasoning_levels, default_reasoning_levels());
        assert_eq!(p.default_reasoning_level, ReasoningEffort::Medium);
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
      base-url: https://api.deepseek.com
      api-key: sk-abc
      timeout: 60
",
        )
        .unwrap();
        assert_eq!(c.models["gpt-4"].timeout, Duration::from_secs(60));
    }

    #[test]
    fn test_rewrite_override() {
        let c = parse(
            "
models:
  legacy-chat:
    provider:
      base-url: https://api.legacy-chat.example
      api-key: sk-abc
    rewrite:
      - at: chat-out
        rename:
          max_completion_tokens: max_tokens
        replace:
          reasoning_effort:
            - if: high
              set: max
              with:
                provider_options.reasoning:
                  enabled: true
      - at: response-out
        remove:
          - parallel_tool_calls
        replace:
          service_tier:
            - if: auto
              set: priority
            - if: priority
              set: default
",
        )
        .unwrap();
        let rewrite = &c.models["legacy-chat"].rewrite;
        assert!(rewrite.responses_in.steps.is_empty());
        assert!(rewrite.chat_in.steps.is_empty());
        assert_eq!(rewrite.chat_out.steps.len(), 2);
        assert_eq!(
            rewrite.chat_out.steps[0],
            RewriteStep::Rename(vec![("max_completion_tokens".into(), "max_tokens".into())])
        );
        assert_eq!(
            rewrite.chat_out.steps[1],
            RewriteStep::Replace(vec![(
                "reasoning_effort".into(),
                vec![ReplaceRule {
                    condition: RewriteCondition::from_value(serde_json::json!("high")).unwrap(),
                    set: Some(serde_json::Value::String("max".into())),
                    with: vec![(
                        "provider_options.reasoning".into(),
                        serde_json::json!({"enabled": true}),
                    )],
                }]
            )])
        );
        assert_eq!(rewrite.responses_out.steps.len(), 2);
        assert_eq!(
            rewrite.responses_out.steps[0],
            RewriteStep::Remove(vec!["parallel_tool_calls".into()])
        );
        assert_eq!(
            rewrite.responses_out.steps[1],
            RewriteStep::Replace(vec![(
                "service_tier".into(),
                vec![
                    ReplaceRule {
                        condition: RewriteCondition::from_value(serde_json::json!("auto")).unwrap(),
                        set: Some(serde_json::Value::String("priority".into())),
                        with: vec![],
                    },
                    ReplaceRule {
                        condition: RewriteCondition::from_value(serde_json::json!("priority"))
                            .unwrap(),
                        set: Some(serde_json::Value::String("default".into())),
                        with: vec![],
                    }
                ]
            )])
        );
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
      base-url: https://api.deepseek.com
      api-key: sk-abc
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
      base-url: https://api.deepseek.com
      api-key: sk-abc
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
  allowed-tool-types: [function, web_search_preview]
  log-level: debug
  compact-encryption-key: abcd

models:
  gpt-4:
    provider:
      base-url: https://api.deepseek.com
      api-key: sk-abc
",
        )
        .unwrap();
        assert_eq!(c.listen, "127.0.0.1:8080");
        assert_eq!(c.timeout, 45);
        assert!(c.auth_enabled());
        assert_eq!(c.auth_keys.len(), 2);
        assert_eq!(c.allowed_tool_types, vec!["function", "web_search_preview"]);
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
      base-url: https://api.deepseek.com
      api-key: $TEST_KEY
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
      base-url: https://api.deepseek.com
      api-key: $MISSING_VAR
",
        )
        .unwrap();
        assert_eq!(c.models["gpt-4"].api_key, "");
    }

    #[test]
    fn test_custom_allowed_tool_types() {
        let c = parse(
            "
server:
  allowed-tool-types: [mcp, function]

models:
  gpt-4:
    provider:
      base-url: https://api.deepseek.com
      api-key: sk-abc
",
        )
        .unwrap();
        assert_eq!(c.allowed_tool_types, vec!["mcp", "function"]);
    }

    #[test]
    fn test_allowed_tool_types_defaults_to_function() {
        let c = parse(
            "
models:
  gpt-4:
    provider:
      base-url: https://api.deepseek.com
      api-key: sk-abc
",
        )
        .unwrap();
        assert_eq!(c.allowed_tool_types, vec!["function"]);
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
      base-url: https://api.deepseek.com
      api-key: sk-abc
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
      base-url: https://api.deepseek.com
      api-key: plain-text-key
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
      base-url: https://api.deepseek.com
      api-key: sk-1
  b:
    provider:
      base-url: https://api.deepseek.com
      api-key: sk-2
",
        )
        .unwrap();
        assert_eq!(c.models.len(), 2);
        assert_eq!(c.model_names.len(), 2);
        assert!(c.model_names.contains(&"a".into()));
        assert!(c.model_names.contains(&"b".into()));
    }
}
