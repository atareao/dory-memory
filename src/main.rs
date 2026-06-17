use std::sync::Arc;

use anyhow::Context;
use sqlx::postgres::PgPoolOptions;
use tower_http::cors::CorsLayer;

mod cache;
mod config;
mod embed;
mod engine;
mod error;
mod guard;
mod models;
mod routes;
mod telemetry;
mod workers;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "dory_memory=info".into()),
        )
        .init();

    let config_path = std::env::var("DORY_CONFIG").unwrap_or_else(|_| "dory.toml".to_string());
    let config = config::DoryConfig::load(&config_path)
        .with_context(|| format!("Failed to load config from {config_path}"))?;

    tracing::info!("Connecting to database...");
    let pool = PgPoolOptions::new()
        .max_connections(config.database.max_connections)
        .connect(&config.database.url)
        .await
        .context("Failed to connect to database")?;

    tracing::info!("Running migrations...");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("Failed to run database migrations")?;

    let engine = Arc::new(engine::DoryEngine::new(pool.clone()));
    let cache = Arc::new(cache::DoryCacheManager::new());
    let embedder = Arc::new(embed::EmbeddingClient::new(
        config.embedding.api_url.clone(),
        config.embedding.api_key.clone(),
        config.embedding.model.clone(),
    ));

    let pool_clone = pool.clone();
    tokio::spawn(async move {
        workers::start_decay_pruning_loop(pool_clone).await;
    });

    let engine_clone = Arc::clone(&engine);
    tokio::spawn(async move {
        workers::start_consolidation_loop(engine_clone).await;
    });

    let workspace_root = std::env::var("DORY_WORKSPACE_ROOT").ok();
    if let Some(root) = workspace_root
        && let Err(e) = telemetry::start_workspace_daemon(&root)
    {
        tracing::warn!("Telemetry daemon failed to start: {e}");
    }

    let app_state = routes::AppState {
        engine,
        cache,
        embedder,
        default_dimensions: config.embedding.dimensions,
        embedding_model: config.embedding.model,
    };

    let app = routes::create_router(app_state).layer(CorsLayer::permissive());

    let addr = format!("{}:{}", config.server.host, config.server.port);
    tracing::info!("Starting server on {addr}");

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("Failed to bind to {addr}"))?;

    axum::serve(listener, app).await.context("Server failed")?;

    Ok(())
}
