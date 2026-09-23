use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Query, State},
    http::StatusCode,
    middleware,
    response::IntoResponse,
    routing::{get, post},
};
use clap::Parser;
use responses_proxy::app;
use responses_proxy::catalog;
use responses_proxy::config;
use responses_proxy::handlers;
use std::collections::HashMap;
use tower_http::cors::{Any, CorsLayer};
use tower_http::decompression::RequestDecompressionLayer;

// ── CLI ──────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "responses-proxy")]
struct Cli {
    #[arg(short, long, default_value_t = app::home_dir().join("config.yaml").display().to_string())]
    config: String,
}

// ── Server entrypoint ────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    app::ensure_dirs();

    let cli = Cli::parse();
    let resolved = config::load_config(&cli.config).expect("Failed to load config");

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| format!("responses_proxy={}", resolved.log_level).into());
    tracing_subscriber::fmt().with_env_filter(env_filter).init();

    tracing::info!(
        "Loaded {} models from {}",
        resolved.models.len(),
        cli.config
    );

    let state = app::State::new(resolved);
    state.store().start_sweep_task();

    // CORS: allow all origins unless explicitly restricted
    let cors = if state.config().cors_allow_origins.is_empty() {
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any)
    } else {
        let origins: Vec<axum::http::HeaderValue> = state
            .config()
            .cors_allow_origins
            .iter()
            .filter_map(|o| {
                o.parse()
                    .map_err(|e| tracing::warn!(origin = %o, error = %e, "Invalid CORS origin"))
                    .ok()
            })
            .collect();
        CorsLayer::new()
            .allow_origin(tower_http::cors::AllowOrigin::list(origins))
            .allow_methods(Any)
            .allow_headers(Any)
    };

    let auth = middleware::from_fn_with_state(state.clone(), handlers::check);
    let listen = state.config().listen.to_string();

    let app = Router::new()
        .route("/health", get(health_check))
        .route("/v1/models", get(list_models).route_layer(auth.clone()))
        .route(
            "/v1/responses",
            get(handlers::websocket)
                .route_layer(auth.clone())
                .post(handlers::responses),
        )
        .route(
            "/v1/responses/compact",
            post(handlers::compact).route_layer(auth.clone()),
        )
        .route(
            "/v1/responses/input_tokens",
            post(handlers::input_tokens).route_layer(auth.clone()),
        )
        .route(
            "/v1/responses/{response_id}/cancel",
            post(handlers::cancel).route_layer(auth.clone()),
        )
        .layer(cors)
        .layer(
            RequestDecompressionLayer::new()
                .gzip(true)
                .br(true)
                .zstd(true)
                .deflate(true),
        )
        // Innermost so the limit applies to the decompressed body the extractor buffers.
        .layer(DefaultBodyLimit::max(state.config().max_body_bytes))
        .with_state(state.clone());

    tracing::info!("Listening on {}", listen);
    let listener = tokio::net::TcpListener::bind(&listen).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// ── Simple health-check endpoint ─────────────────────────────────────────

async fn health_check() -> &'static str {
    "OK"
}

// ── OpenAI-compatible model listing ──────────────────────────────────────

async fn list_models(
    State(state): State<app::State>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    // Codex appends `?client_version=...` and reads its context window from a
    // proprietary `{"models":[...]}` schema; plain OpenAI clients get the
    // standard `{"object":"list","data":[...]}` shape.
    if params.contains_key("client_version") {
        return Ok(Json(codex_model_list(&state).await));
    }

    let data: Vec<serde_json::Value> = state
        .config()
        .models
        .keys()
        .map(|name| {
            serde_json::json!({
                "id": name, "object": "model", "created": 0, "owned_by": "responses-proxy"
            })
        })
        .collect();
    Ok(Json(serde_json::json!({"object": "list", "data": data})))
}

/// Build Codex's proprietary model-list response. Codex ignores the OpenAI
/// shape, so this mirrors its `ModelsResponse`/`ModelInfo` schema and carries
/// the per-model `context_window` (config override → upstream value → null,
/// where null lets Codex fall back to its bundled default). Every required
/// field must be present or Codex silently discards the entry.
async fn codex_model_list(state: &app::State) -> serde_json::Value {
    let mut models = Vec::new();
    for (name, provider) in &state.config().models {
        let context_window = state.resolve_context_window(provider).await;
        models.push(catalog::codex_model_entry(name, provider, context_window));
    }
    serde_json::json!({ "models": models })
}
